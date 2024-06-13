use crate::{db_source, RunArgs};
use anyhow::bail;
use arroyo_openapi::types::{PipelinePatch, PipelinePost, StopType, ValidateQueryPost};
use arroyo_openapi::Client;
use arroyo_rpc::config;
use arroyo_rpc::config::{DatabaseType, Scheduler};
use arroyo_server_common::log_event;
use arroyo_server_common::shutdown::{Shutdown, ShutdownHandler};
use async_trait::async_trait;
use rand::random;
use serde_json::json;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

async fn wait_for_state(
    client: &Client,
    pipeline_id: &str,
    expected_state: &str,
) -> anyhow::Result<()> {
    let mut last_state = "None".to_string();
    while last_state != expected_state {
        let jobs = client
            .get_pipeline_jobs()
            .id(pipeline_id)
            .send()
            .await
            .unwrap();
        let job = jobs.data.first().unwrap();

        let state = job.state.clone();
        if last_state != state {
            info!("Job transitioned to {}", state);
            last_state = state;
        }

        if last_state == "Failed" {
            bail!("Job transitioned to failed");
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

async fn wait_for_connect(client: &Client) -> anyhow::Result<()> {
    for _ in 0..50 {
        if client.ping().send().await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    bail!("API server did not start up successfully; see logs for more details");
}

struct PipelineShutdownHandler {
    client: Arc<Client>,
    pipeline_id: String,
}

#[async_trait]
impl ShutdownHandler for PipelineShutdownHandler {
    async fn shutdown(&self) {
        if let Err(e) = self
            .client
            .patch_pipeline()
            .id(&self.pipeline_id)
            .body(PipelinePatch::builder().stop(StopType::Checkpoint))
            .send()
            .await
        {
            warn!("Unable to stop pipeline with a final checkpoint: {}", e);
        }
    }
}

pub async fn run(args: RunArgs) {
    let _guard = arroyo_server_common::init_logging("pipeline");

    let query = std::io::read_to_string(args.query).unwrap();

    let mut shutdown = Shutdown::new("pipeline");

    let db_path = args.database.clone().unwrap_or_else(|| {
        PathBuf::from_str(&format!("/tmp/arroyo/{}.arroyo", random::<u32>())).unwrap()
    });

    info!("Using database {}", db_path.to_string_lossy());

    config::update(|c| {
        c.database.r#type = DatabaseType::Sqlite;
        c.database.sqlite.path = db_path.clone();

        c.api.http_port = 0;
        c.controller.rpc_port = 0;
        c.controller.scheduler = Scheduler::Embedded;
    });

    let db = db_source().await;

    log_event("pipeline_cluster_start", json!({}));

    let controller_port = arroyo_controller::ControllerServer::new(db.clone())
        .await
        .start(shutdown.guard("controller"))
        .await
        .expect("could not start system");

    config::update(|c| c.controller.rpc_port = controller_port);

    let http_port = arroyo_api::start_server(db.clone(), shutdown.guard("api")).unwrap();

    let client = Arc::new(Client::new_with_client(
        &format!("http://localhost:{http_port}/api",),
        reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap(),
    ));

    // wait until server is available
    wait_for_connect(&client).await.unwrap();

    // validate the pipeline
    let errors = client
        .validate_query()
        .body(ValidateQueryPost::builder().query(&query))
        .send()
        .await
        .expect("Something went wrong while running pipeline")
        .into_inner();

    if !errors.errors.is_empty() {
        eprintln!("There were some issues with the provided query");
        for error in errors.errors {
            eprintln!("  * {error}");
        }
        exit(1);
    }

    let id = client
        .create_pipeline()
        .body(
            PipelinePost::builder()
                .name(args.name.unwrap_or_else(|| "query".to_string()))
                .parallelism(1)
                .query(&query),
        )
        .send()
        .await
        .unwrap()
        .into_inner()
        .id;

    wait_for_state(&client, &id, "Running").await.unwrap();

    info!("Pipeline running... dashboard at http://localhost:{http_port}/pipelines/{id}");

    shutdown.set_handler(Box::new(PipelineShutdownHandler {
        client,
        pipeline_id: id,
    }));

    Shutdown::handle_shutdown(shutdown.wait_for_shutdown(Duration::from_secs(60)).await);
}
