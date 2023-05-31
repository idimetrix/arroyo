// @generated by protoc-gen-connect-web v0.2.1 with parameter "target=ts"
// @generated from file api.proto (package arroyo_api, syntax proto3)
/* eslint-disable */
/* @ts-nocheck */

import {CheckpointDetailsReq, CheckpointDetailsResp, ConfluentSchemaReq, ConfluentSchemaResp, CreateConnectionReq, CreateConnectionResp, CreateJobReq, CreateJobResp, CreatePipelineReq, CreatePipelineResp, CreateSinkReq, CreateSinkResp, CreateSourceReq, CreateSourceResp, DeleteConnectionReq, DeleteConnectionResp, DeleteJobReq, DeleteJobResp, DeleteSinkReq, DeleteSinkResp, DeleteSourceReq, DeleteSourceResp, GetConnectionsReq, GetConnectionsResp, GetJobsReq, GetJobsResp, GetPipelineReq, GetSinksReq, GetSinksResp, GetSourcesReq, GetSourcesResp, GrpcOutputSubscription, JobCheckpointsReq, JobCheckpointsResp, JobDetailsReq, JobDetailsResp, JobMetricsReq, JobMetricsResp, OperatorErrorsReq, OperatorErrorsRes, OutputData, PipelineDef, PipelineGraphReq, PipelineGraphResp, SourceMetadataResp, TestSchemaResp, TestSourceMessage, UpdateJobReq, UpdateJobResp} from "./api_pb.js";
import {MethodKind} from "@bufbuild/protobuf";

/**
 * @generated from service arroyo_api.ApiGrpc
 */
export const ApiGrpc = {
  typeName: "arroyo_api.ApiGrpc",
  methods: {
    /**
     * @generated from rpc arroyo_api.ApiGrpc.CreateConnection
     */
    createConnection: {
      name: "CreateConnection",
      I: CreateConnectionReq,
      O: CreateConnectionResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.TestConnection
     */
    testConnection: {
      name: "TestConnection",
      I: CreateConnectionReq,
      O: TestSourceMessage,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetConnections
     */
    getConnections: {
      name: "GetConnections",
      I: GetConnectionsReq,
      O: GetConnectionsResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.DeleteConnection
     */
    deleteConnection: {
      name: "DeleteConnection",
      I: DeleteConnectionReq,
      O: DeleteConnectionResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.CreateSource
     */
    createSource: {
      name: "CreateSource",
      I: CreateSourceReq,
      O: CreateSourceResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetSources
     */
    getSources: {
      name: "GetSources",
      I: GetSourcesReq,
      O: GetSourcesResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.DeleteSource
     */
    deleteSource: {
      name: "DeleteSource",
      I: DeleteSourceReq,
      O: DeleteSourceResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.CreateSink
     */
    createSink: {
      name: "CreateSink",
      I: CreateSinkReq,
      O: CreateSinkResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetSinks
     */
    getSinks: {
      name: "GetSinks",
      I: GetSinksReq,
      O: GetSinksResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.DeleteSink
     */
    deleteSink: {
      name: "DeleteSink",
      I: DeleteSinkReq,
      O: DeleteSinkResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetConfluentSchema
     */
    getConfluentSchema: {
      name: "GetConfluentSchema",
      I: ConfluentSchemaReq,
      O: ConfluentSchemaResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetSourceMetadata
     */
    getSourceMetadata: {
      name: "GetSourceMetadata",
      I: CreateSourceReq,
      O: SourceMetadataResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.TestSchema
     */
    testSchema: {
      name: "TestSchema",
      I: CreateSourceReq,
      O: TestSchemaResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.TestSource
     */
    testSource: {
      name: "TestSource",
      I: CreateSourceReq,
      O: TestSourceMessage,
      kind: MethodKind.ServerStreaming,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.CreatePipeline
     */
    createPipeline: {
      name: "CreatePipeline",
      I: CreatePipelineReq,
      O: CreatePipelineResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GraphForPipeline
     */
    graphForPipeline: {
      name: "GraphForPipeline",
      I: PipelineGraphReq,
      O: PipelineGraphResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetPipeline
     */
    getPipeline: {
      name: "GetPipeline",
      I: GetPipelineReq,
      O: PipelineDef,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.CreateJob
     */
    createJob: {
      name: "CreateJob",
      I: CreateJobReq,
      O: CreateJobResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.DeleteJob
     */
    deleteJob: {
      name: "DeleteJob",
      I: DeleteJobReq,
      O: DeleteJobResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.StartPipeline
     */
    startPipeline: {
      name: "StartPipeline",
      I: CreatePipelineReq,
      O: CreateJobResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.PreviewPipeline
     */
    previewPipeline: {
      name: "PreviewPipeline",
      I: CreatePipelineReq,
      O: CreateJobResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetJobs
     */
    getJobs: {
      name: "GetJobs",
      I: GetJobsReq,
      O: GetJobsResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetJobDetails
     */
    getJobDetails: {
      name: "GetJobDetails",
      I: JobDetailsReq,
      O: JobDetailsResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetCheckpoints
     */
    getCheckpoints: {
      name: "GetCheckpoints",
      I: JobCheckpointsReq,
      O: JobCheckpointsResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetCheckpointDetail
     */
    getCheckpointDetail: {
      name: "GetCheckpointDetail",
      I: CheckpointDetailsReq,
      O: CheckpointDetailsResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetOperatorErrors
     */
    getOperatorErrors: {
      name: "GetOperatorErrors",
      I: OperatorErrorsReq,
      O: OperatorErrorsRes,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.GetJobMetrics
     */
    getJobMetrics: {
      name: "GetJobMetrics",
      I: JobMetricsReq,
      O: JobMetricsResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.UpdateJob
     */
    updateJob: {
      name: "UpdateJob",
      I: UpdateJobReq,
      O: UpdateJobResp,
      kind: MethodKind.Unary,
    },
    /**
     * @generated from rpc arroyo_api.ApiGrpc.SubscribeToOutput
     */
    subscribeToOutput: {
      name: "SubscribeToOutput",
      I: GrpcOutputSubscription,
      O: OutputData,
      kind: MethodKind.ServerStreaming,
    },
  }
} as const;

