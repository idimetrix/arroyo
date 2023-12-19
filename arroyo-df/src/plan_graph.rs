use std::{
    collections::{HashMap, HashSet},
    io::sink,
    sync::Arc,
    time::Duration,
};

use arrow_schema::{DataType, Schema};
use arroyo_datastream::{
    EdgeType, ExpressionReturnType, NonWindowAggregator, Operator, PeriodicWatermark, Program,
    ProgramUdf, SlidingAggregatingTopN, SlidingWindowAggregator, Stream, StreamEdge, StreamNode,
    TumblingTopN, TumblingWindowAggregator, WatermarkStrategy, WindowAgg, WindowType,
};

use datafusion::{
    datasource::MemTable,
    execution::{
        context::{SessionConfig, SessionState},
        runtime_env::RuntimeEnv,
    },
    physical_plan::{memory::MemoryExec, streaming::StreamingTableExec, PhysicalExpr},
    physical_planner::{DefaultPhysicalPlanner, PhysicalPlanner},
};
use petgraph::{
    data::Build,
    graph::{DiGraph, NodeIndex},
    visit::{IntoNeighborsDirected, Topo},
};
use quote::{quote, ToTokens};
use syn::{parse_quote, parse_str, Type};
use tracing::{info, warn};

use crate::QueryToGraphVisitor;
use crate::{
    tables::Table,
    types::{StructDef, StructField, StructPair, TypeDef},
    ArroyoSchemaProvider, CompiledSql, EmptyPartitionStream, SqlConfig,
};
use anyhow::{anyhow, bail, Context, Result};
use arroyo_datastream::EdgeType::Forward;
use arroyo_rpc::grpc::api::{
    window, KeyPlanOperator, MemTableScan, ProjectionOperator, TumblingWindow, ValuePlanOperator,
    Window, WindowAggregateOperator,
};
use datafusion_common::{DFField, DFSchema, DFSchemaRef, DataFusionError, ScalarValue};
use datafusion_expr::{logical_plan, BinaryExpr, Cast, Expr, LogicalPlan};
use datafusion_proto::{
    bytes::{physical_plan_to_bytes, Serializeable},
    physical_plan::{
        to_proto, AsExecutionPlan, DefaultPhysicalExtensionCodec, PhysicalExtensionCodec,
    },
    protobuf::{PhysicalExprNode, PhysicalPlanNode},
};
use petgraph::Direction;
use prost::Message;

#[derive(Debug)]
pub struct DebugPhysicalExtensionCodec {}

impl PhysicalExtensionCodec for DebugPhysicalExtensionCodec {
    fn try_decode(
        &self,
        buf: &[u8],
        inputs: &[Arc<dyn datafusion::physical_plan::ExecutionPlan>],
        registry: &dyn datafusion::execution::FunctionRegistry,
    ) -> datafusion_common::Result<Arc<dyn datafusion::physical_plan::ExecutionPlan>> {
        todo!()
    }

    fn try_encode(
        &self,
        node: Arc<dyn datafusion::physical_plan::ExecutionPlan>,
        buf: &mut Vec<u8>,
    ) -> datafusion_common::Result<()> {
        let mem_table: Option<&EmptyPartitionStream> = node.as_any().downcast_ref();
        if let Some(table) = mem_table {
            serde_json::to_writer(buf, table).map_err(|err| {
                DataFusionError::Internal(format!(
                    "couldn't serialize empty partition stream {}",
                    err
                ))
            })?;
            return Ok(());
        }
        Err(DataFusionError::Internal(format!(
            "cannot serialize {:?}",
            node
        )))
    }
}

pub(crate) async fn get_arrow_program(
    mut rewriter: QueryToGraphVisitor,
    schema_provider: ArroyoSchemaProvider,
) -> Result<CompiledSql> {
    warn!(
        "graph is {:?}",
        petgraph::dot::Dot::with_config(&rewriter.local_logical_plan_graph, &[])
    );
    let mut topo = Topo::new(&rewriter.local_logical_plan_graph);
    let mut program_graph: DiGraph<StreamNode, StreamEdge> = DiGraph::new();

    let planner = DefaultPhysicalPlanner::default();
    let mut config = SessionConfig::new();
    config
        .options_mut()
        .optimizer
        .enable_round_robin_repartition = false;
    config.options_mut().optimizer.repartition_aggregations = false;
    let session_state = SessionState::with_config_rt(config, Arc::new(RuntimeEnv::default()));

    let mut node_mapping = HashMap::new();
    while let Some(node_index) = topo.next(&rewriter.local_logical_plan_graph) {
        let logical_extension = rewriter
            .local_logical_plan_graph
            .node_weight(node_index)
            .unwrap();
        match logical_extension {
            crate::LogicalPlanExtension::TableScan(logical_plan) => {
                let LogicalPlan::TableScan(table_scan) = logical_plan else {
                    bail!("expected table scan")
                };

                let table_name = table_scan.table_name.to_string();
                let source = schema_provider
                    .get_table(&table_name)
                    .ok_or_else(|| anyhow!("table {} not found", table_scan.table_name))?;

                let Table::ConnectorTable(cn) = source else {
                    bail!("expect connector table")
                };
                let sql_source = cn.as_sql_source()?;
                let source_index = program_graph.add_node(StreamNode {
                    operator_id: format!("source_{}", program_graph.node_count()),
                    operator: sql_source.source.operator,
                    parallelism: 1,
                });
                let watermark_index = program_graph.add_node(StreamNode {
                    operator_id: format!("watermark_{}", program_graph.node_count()),
                    operator: Operator::ArrowWatermark,
                    parallelism: 1,
                });
                program_graph.add_edge(
                    source_index,
                    watermark_index,
                    StreamEdge {
                        key: "()".into(),
                        value: "()".into(),
                        typ: EdgeType::Forward,
                    },
                );
                node_mapping.insert(node_index, watermark_index);
            }
            crate::LogicalPlanExtension::ValueCalculation(logical_plan) => {
                let inputs = logical_plan.inputs();
                let physical_plan = planner
                    .create_physical_plan(logical_plan, &session_state)
                    .await;

                let physical_plan =
                    physical_plan.context("creating physical plan for value calculation")?;

                let physical_plan_node: PhysicalPlanNode =
                    PhysicalPlanNode::try_from_physical_plan(
                        physical_plan,
                        &DebugPhysicalExtensionCodec {},
                    )?;
                let config = ValuePlanOperator {
                    name: "tmp".into(),
                    physical_plan: physical_plan_node.encode_to_vec(),
                };

                let new_node_index = program_graph.add_node(StreamNode {
                    operator_id: format!("value_{}", program_graph.node_count()),
                    operator: Operator::ArrowValue {
                        name: "arrow_value".into(),
                        config: config.encode_to_vec(),
                    },
                    parallelism: 1,
                });
                node_mapping.insert(node_index, new_node_index);
                for upstream in rewriter
                    .local_logical_plan_graph
                    .neighbors_directed(node_index, Direction::Incoming)
                {
                    program_graph.add_edge(
                        *node_mapping.get(&upstream).unwrap(),
                        new_node_index,
                        StreamEdge {
                            key: "()".to_string(),
                            value: "()".to_string(),
                            typ: EdgeType::Forward,
                        },
                    );
                }
            }
            crate::LogicalPlanExtension::KeyCalculation {
                projection: logical_plan,
                key_columns,
            } => {
                info!("logical plan for key calculation:\n{:?}", logical_plan);
                info!("input schema: {:?}", logical_plan.schema());
                let physical_plan = planner
                    .create_physical_plan(logical_plan, &session_state)
                    .await;

                let physical_plan = physical_plan.context("creating physical plan")?;

                println!("physical plan {:#?}", physical_plan);
                let physical_plan_node: PhysicalPlanNode =
                    PhysicalPlanNode::try_from_physical_plan(
                        physical_plan,
                        &DebugPhysicalExtensionCodec {},
                    )?;
                let config = KeyPlanOperator {
                    name: "tmp".into(),
                    physical_plan: physical_plan_node.encode_to_vec(),
                    key_fields: key_columns.iter().map(|column| (*column) as u64).collect(),
                };

                let new_node_index = program_graph.add_node(StreamNode {
                    operator_id: format!("key_{}", program_graph.node_count()),
                    operator: Operator::ArrowKey {
                        name: "arrow_key".into(),
                        config: config.encode_to_vec(),
                    },
                    parallelism: 1,
                });
                node_mapping.insert(node_index, new_node_index);
                for upstream in rewriter
                    .local_logical_plan_graph
                    .neighbors_directed(node_index, Direction::Incoming)
                {
                    program_graph.add_edge(
                        *node_mapping.get(&upstream).unwrap(),
                        new_node_index,
                        StreamEdge {
                            key: "()".to_string(),
                            value: "()".to_string(),
                            typ: EdgeType::Forward,
                        },
                    );
                }
            }
            crate::LogicalPlanExtension::AggregateCalculation(aggregate) => {
                let WindowType::Tumbling { width } = aggregate.window else {
                    bail!("only implemented tumbling windows currently")
                };
                let mut my_aggregate = aggregate.aggregate.clone();
                let logical_plan = LogicalPlan::Aggregate(my_aggregate);

                let LogicalPlan::TableScan(table_scan) = aggregate.aggregate.input.as_ref() else {
                    bail!("expected logical plan")
                };

                let physical_plan = planner
                    .create_physical_plan(&logical_plan, &session_state)
                    .await
                    .context("couldn't create physical plan for aggregate")?;
                println!("physical plan for aggregate: {:#?}", physical_plan);
                let physical_plan_node: PhysicalPlanNode =
                    PhysicalPlanNode::try_from_physical_plan(
                        physical_plan,
                        &DebugPhysicalExtensionCodec {},
                    )?;

                let division = Expr::BinaryExpr(BinaryExpr {
                    left: Box::new(Expr::Column(datafusion_common::Column {
                        relation: None,
                        name: "timestamp_nanos".into(),
                    })),
                    op: datafusion_expr::Operator::Divide,
                    right: Box::new(Expr::Literal(ScalarValue::Int64(Some(
                        width.as_nanos() as i64
                    )))),
                });
                let timestamp_nanos_field =
                    DFField::new_unqualified("timestamp_nanos", DataType::Int64, false);
                let binning_df_schema =
                    DFSchema::new_with_metadata(vec![timestamp_nanos_field], HashMap::new())
                        .context("can't make timestamp nanos schema")?;
                let binning_arrow_schema: Schema = (&binning_df_schema).into();
                let binning_function = planner
                    .create_physical_expr(
                        &division,
                        &binning_df_schema,
                        &binning_arrow_schema,
                        &session_state,
                    )
                    .context("couldn't create binning function")?;
                let binning_function_proto = PhysicalExprNode::try_from(binning_function)
                    .context("couldn't encode binning function")?;
                let input_schema: Schema = aggregate.aggregate.input.schema().as_ref().into();

                let config = WindowAggregateOperator {
                    name: "window_aggregate".into(),
                    physical_plan: physical_plan_node.encode_to_vec(),
                    binning_function: binning_function_proto.encode_to_vec(),
                    binning_schema: serde_json::to_vec(&binning_arrow_schema)?,
                    input_schema: serde_json::to_vec(&input_schema)?,
                    window: Some(Window {
                        window: Some(window::Window::TumblingWindow(TumblingWindow {
                            size_micros: width.as_micros() as u64,
                        })),
                    }),
                    window_field_name: aggregate.window_field.name().to_string(),
                    window_index: aggregate.window_index as u64,
                    key_fields: aggregate
                        .key_fields
                        .iter()
                        .map(|field| (*field) as u64)
                        .collect(),
                };
                let new_node_index = program_graph.add_node(StreamNode {
                    operator_id: format!("aggregate_{}", program_graph.node_count()),
                    operator: Operator::ArrowAggregate {
                        name: "arrow_aggregate".into(),
                        config: config.encode_to_vec(),
                    },
                    parallelism: 1,
                });
                node_mapping.insert(node_index, new_node_index);
                for upstream in rewriter
                    .local_logical_plan_graph
                    .neighbors_directed(node_index, Direction::Incoming)
                {
                    program_graph.add_edge(
                        *node_mapping.get(&upstream).unwrap(),
                        new_node_index,
                        StreamEdge {
                            key: "()".into(),
                            value: "()".into(),
                            typ: EdgeType::Shuffle,
                        },
                    );
                }
            }
            crate::LogicalPlanExtension::Sink => {
                let sink_index = program_graph.add_node(StreamNode {
                    operator_id: format!("sink_{}", program_graph.node_count()),
                    operator: Operator::RecordBatchGrpc,
                    parallelism: 1,
                });
                node_mapping.insert(node_index, sink_index);
                for upstream in rewriter
                    .local_logical_plan_graph
                    .neighbors_directed(node_index, Direction::Incoming)
                {
                    program_graph.add_edge(
                        *node_mapping.get(&upstream).unwrap(),
                        sink_index,
                        StreamEdge {
                            key: "()".to_string(),
                            value: "()".to_string(),
                            typ: EdgeType::Forward,
                        },
                    );
                }
            }
        }
    }

    let program = Program {
        types: vec![],
        udfs: vec![],
        other_defs: vec![],
        graph: program_graph,
    };
    Ok(CompiledSql {
        program,
        connection_ids: vec![],
        schemas: HashMap::new(),
    })
}
