import { ServaManagerClient } from "./generated/ApiServiceClientPb";
import {
  Address,
  GetConfigRequest,
  ListDirRequest,
  ManageDirOrFileRequest,
  UploadFileChunkRequest
} from "./generated/api_pb.js";
import Operation = ManageDirOrFileRequest.Operation;

const IS_DEVELOPMENT = process.env.NODE_ENV === "development";
const DEBUG_SERVER = "http://localhost:3000";
//ATTENTION: debug code mixed
const client = IS_DEVELOPMENT
  ? new ServaManagerClient(DEBUG_SERVER)
  : new ServaManagerClient(window.location.origin);

export type ConvertDirFunction = (path: string, dateModified: number) => any;
export type ConvertFileFunction = (path: string, dateModified: number, size: number) => any;
export type Permission = {
  create: boolean;
  copy: boolean;
  move: boolean;
  delete: boolean;
  rename: boolean;
  upload: boolean;
  download: boolean;
};
export type Config = {
  root: string;
  root_canonical: string;
  prefix: string;
  addressList: Address.AsObject[];
  permission: Permission;
};
let config: Config | undefined = undefined;
export { Operation };

export async function ListDir(
  dir_path: string,
  fn_dir: ConvertDirFunction,
  fn_file: ConvertFileFunction
) {
  let request = new ListDirRequest();
  request.setDirPath(dir_path);
  let list_dir_result = await client.listDir(request, null);
  let directories = list_dir_result
    .getDirectoriesList()
    .map((dir) => fn_dir(dir.getPath(), dir.getModifiedTimestampInMs()));
  let files = list_dir_result
    .getFilesList()
    .map((file) => fn_file(file.getPath(), file.getModifiedTimestampInMs(), file.getSize()));
  return [...directories, ...files];
}

export async function GetConfig(): Promise<Config> {
  if (config) {
    return config;
  }
  let response = await client.getConfig(new GetConfigRequest(), null);
  let permission = response.getPermission();
  if (!permission) {
    throw new Error("Missing mandatory permission in GetConfigResponse!");
  }
  config = {
    root: response.getRoot(),
    root_canonical: response.getRootCanonical(),
    prefix: response.getPrefix(),
    addressList: response.getAddressList().map((addr) => {
      return { host: addr.getHost(), port: addr.getPort() };
    }),
    permission: {
      create: permission.getCreate(),
      copy: permission.getCopy(),
      move: permission.getMove(),
      delete: permission.getDelete(),
      rename: permission.getRename(),
      upload: permission.getUpload(),
      download: permission.getDownload(),
    },
  };
  return config;
}

export async function UploadFileChunk(
  dir_path: string,
  file_name: string,
  file_size: number,
  file_hash: string,
  abort: boolean,
  chunk_blob: Blob,
  chunk_id: number,
  chunk_count: number,
  chunk_offset: number,
  chunk_size: number,
  chunk_hash: string
) {
  let config = await GetConfig();
  if (!config.permission.upload) {
    throw new Error("Upload is not allowed!");
  }
  let chunk_array = await chunk_blob.arrayBuffer();
  let chunk_data = new Uint8Array(chunk_array);
  let request = new UploadFileChunkRequest();
  request.setDirPath(dir_path);
  request.setFileName(file_name);
  request.setFileSize(file_size);
  request.setFileHash(file_hash);
  request.setAbort(abort);
  request.setChunkData(chunk_data);
  request.setChunkId(chunk_id);
  request.setChunkCount(chunk_count);
  request.setChunkOffset(chunk_offset);
  request.setChunkSize(chunk_size);
  request.setChunkHash(chunk_hash);
  await client.uploadFileChunk(request, null);
}

export async function ManageDirOrFile(
  file_name_path: string,
  dir_path: string,
  target: string,
  operation: Operation
) {
  let config = await GetConfig();
  let allowed = false;
  switch (operation) {
    case Operation.CREATE_DIR:
      allowed = config.permission.create;
      break;
    case Operation.COPY_FILE:
      allowed = config.permission.copy;
      break;
    case Operation.DELETE_FILE:
      allowed = config.permission.delete;
      break;
    case Operation.MOVE_FILE:
      allowed = config.permission.move;
      break;
    case Operation.RENAME_FILE:
      allowed = config.permission.rename;
      break;
    default:
  }
  if (!allowed) {
    throw new Error(`Operation ${operation} is not allowed!`);
  }
  let request = new ManageDirOrFileRequest();
  request.setFilePathName(file_name_path);
  request.setDirPath(dir_path);
  request.setTarget(target);
  request.setOperation(operation);
  await client.manageDirOrFile(request, null);
}
