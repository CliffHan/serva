import CustomFileSystemProvider, { Options } from 'devextreme/file_management/custom_provider';
import FileSystemError from 'devextreme/file_management/error';
import FileSystemItem from 'devextreme/file_management/file_system_item';
import UploadInfo from 'devextreme/file_management/upload_info';
import { EventEmitter } from 'eventemitter3';
// import * from 'devextreme/file_management/utils';
import {
  ConvertDirFunction,
  ConvertFileFunction,
  GetConfig,
  ListDir,
  ManageDirOrFile,
  Operation,
  UploadFileChunk
} from './GrpcClient';

const IS_DEVELOPMENT = process.env.NODE_ENV === 'development';
const DEBUG_SERVER = 'http://localhost:3000';

export const EventBus = new EventEmitter<string, EventContextDownload>();
export const EVENT_DOWNLOAD = 'event-download';
export type EventContextDownload = {
  state: number,
  succeeded: boolean | undefined,
  current: number,
  total: number,
}
export enum DownloadState {
  STARTING = 0,
  STEPPING = 1,
  STOPPING = 2,
}

// refer to files in 'devextreme/file_management/error_codes/package.json', no ts provided
// Values:
//   NoAccess: 0,
//   FileExists: 1,
//   FileNotFound: 2,
//   DirectoryExists: 3,
//   DirectoryNotFound: 4,
//   WrongFileExtension: 5,
//   MaxFileSizeExceeded: 6,
//   InvalidSymbols: 7,
//   Other: 32767
// Only Other used because backend don't follow these error codes
const ERROR_OTHER = 32767;

function getFileSystemErrorFromError(e: unknown, fileSystemItem?: FileSystemItem | undefined) {
  let message = e instanceof Error ? e.message : 'Unknown error';
  return new FileSystemError(ERROR_OTHER, fileSystemItem, message);
}

function getFileSystemErrorFromMsg(message: string, fileSystemItem?: FileSystemItem | undefined) {
  return new FileSystemError(ERROR_OTHER, fileSystemItem, message);
}

async function execUploadFileChunk(
  dir_path: string,
  file_name: string,
  file_size: number,
  chunk_blob: Blob,
  chunk_id: number,
  chunk_count: number,
  chunk_offset: number,
  chunk_size: number
) {
  await UploadFileChunk(
    dir_path,
    file_name,
    file_size,
    '',
    false,
    chunk_blob,
    chunk_id,
    chunk_count,
    chunk_offset,
    chunk_size,
    ''
  );
}

async function execAbortFileChunk(dir_path: string, file_name: string, file_size: number) {
  await UploadFileChunk(dir_path, file_name, file_size, '', true, new Blob(), 0, 0, 0, 0, '');
}

// refer to 'CustomFileSystemProviderOptions' in 'devextreme/file_management/custom_provider'
class AbstractOptions implements Options {
  async abortFileUpload(file: File, uploadInfo: UploadInfo, destinationDirectory: FileSystemItem) {
    // => PromiseLike<any> | any;
    console.log('abortFileUpload');
    throw getFileSystemErrorFromMsg('Not implemented');
  }

  async copyItem(item: FileSystemItem, destinationDirectory: FileSystemItem) {
    // => PromiseLike<any> | any
    console.log('copyItem');
    throw getFileSystemErrorFromMsg('Not implemented');
  }

  async createDirectory(parentDirectory: FileSystemItem, name: string) {
    // => PromiseLike<any> | any
    console.log('createDirectory');
    throw getFileSystemErrorFromMsg('Not implemented');
  }

  async deleteItem(item: FileSystemItem) {
    // => PromiseLike<any> | any
    console.log('deleteItem');
    throw getFileSystemErrorFromMsg('Not implemented');
  }

  async downloadItems(items: Array<FileSystemItem>) {
    // => void
    console.log(`downloadItems(), items=${JSON.stringify(items, null, 4)}`);
    throw getFileSystemErrorFromMsg('Not implemented');
  }

  async getItems(parentDirectory: FileSystemItem): Promise<Array<any>> {
    // => PromiseLike<Array<any>> | Array<any>;
    console.log(`getItems(), parentDirectory=${parentDirectory.key}`);
    throw getFileSystemErrorFromMsg('Not implemented');
  }

  async getItemsContent(items: Array<FileSystemItem>) {
    // => PromiseLike<any> | any
    console.log('getItemsContent');
    throw getFileSystemErrorFromMsg('Not implemented');
  }

  //  hasSubDirectoriesExpr?: string | Function;
  //  hasSubDirectoriesExpr() {
  //      console.log('hasSubDirectoriesExpr');
  //  }

  async moveItem(item: FileSystemItem, destinationDirectory: FileSystemItem) {
    // => PromiseLike<any> | any
    console.log('moveItem');
    throw getFileSystemErrorFromMsg('Not implemented');
  }

  async renameItem(item: FileSystemItem, newName: string) {
    // => PromiseLike<any> | any
    console.log('renameItem');
    throw getFileSystemErrorFromMsg('Not implemented');
  }

  async uploadFileChunk(file: File, uploadInfo: UploadInfo, destinationDirectory: FileSystemItem) {
    //  => PromiseLike<any> | any
    console.log(
      `uploadFileChunk(), file=${JSON.stringify(file)}, uploadInfo=${JSON.stringify(
        uploadInfo
      )}, destinationDirectory=${destinationDirectory.key}`
    );
    console.log(
      `uploadFileChunk(), file.name=${file.name}, file.type=${file.type}, file.size=${file.size}`
    );
    console.log(
      `uploadFileChunk(), uploadInfo.chunkBlob, type=${uploadInfo.chunkBlob.type}, size=${uploadInfo.chunkBlob.size}`
    );
    throw getFileSystemErrorFromMsg('Not implemented');
  }
}

class ServaFileSystemOptions extends AbstractOptions {
  async downloadItems(items: Array<FileSystemItem>) {
    console.log('downloadItems()');

    let total = items.length;
    let current = 0;

    // send an event of start downloading
    EventBus.emit(EVENT_DOWNLOAD, { state: DownloadState.STARTING, current, total });
    let config = await GetConfig();
    if (!config.permission.download) {
      // send an event of stop downloading
      EventBus.emit(EVENT_DOWNLOAD, { state: DownloadState.STOPPING, succeeded: false, current, total });
      throw getFileSystemErrorFromMsg('no permission to download');
    }

    //ATTENTION: debug code mixed
    let urlbase = IS_DEVELOPMENT ? (DEBUG_SERVER + config.prefix) : (window.origin + config.prefix);

    // Since files are shared via GET method, just open a new window to download
    // When multiple files need to be downloaded, 2nd and later request may very likely be blocked
    // Refer to https://stackoverflow.com/a/2587692
    let url_array = items.map((item) => urlbase + item.key);
    for (let i = 0; i < url_array.length; i++) {
      let new_window = window.open(url_array[i]);
      if (new_window) {
        console.log(`succeeded downloading ${url_array[i]}`);
        url_array.splice(i, 1);
        current++;
      } else {
        console.log(`failed to download, left url_array=${JSON.stringify(url_array, null, 4)}`);
        break;
      }
      EventBus.emit(EVENT_DOWNLOAD, { state: DownloadState.STEPPING, current, total });
    }

    let succeeded = (url_array.length === 0);
    // send an event of stop downloading
    EventBus.emit(EVENT_DOWNLOAD, { state: DownloadState.STOPPING, succeeded, current, total });
    if (!succeeded) {
      throw getFileSystemErrorFromMsg('Blocked by browser.');
    }
  }

  async getItems(parentDirectory: FileSystemItem) {
    // => PromiseLike<Array<any>> | Array<any>;
    console.log(`getItems(), parentDirectory=${parentDirectory.key}`);
    let fn_dir: ConvertDirFunction = (path, dateModifiedInMs) => {
      let item = new FileSystemItem(path, true);
      item.dateModified = new Date(dateModifiedInMs);
      return item;
    };
    let fn_file: ConvertFileFunction = (path, dateModifiedInMs, size) => {
      let item = new FileSystemItem(path, false);
      item.dateModified = new Date(dateModifiedInMs);
      item.size = size;
      return item;
    };
    try {
      return await ListDir(parentDirectory.key, fn_dir, fn_file);
    } catch (e: unknown) {
      throw getFileSystemErrorFromError(e);
    }
  }

  async abortFileUpload(file: File, uploadInfo: UploadInfo, destinationDirectory: FileSystemItem) {
    // => PromiseLike<any> | any;
    console.log(
      `abortFileUpload(), file=${JSON.stringify(file)}, uploadInfo=${JSON.stringify(
        uploadInfo
      )}, destinationDirectory=${JSON.stringify(destinationDirectory)}`
    );
    try {
      await execAbortFileChunk(destinationDirectory.key, file.name, file.size);
    } catch (e: unknown) {
      throw getFileSystemErrorFromError(e);
    }
  }

  async uploadFileChunk(file: File, uploadInfo: UploadInfo, destinationDirectory: FileSystemItem) {
    //  => PromiseLike<any> | any
    console.log(
      `uploadFileChunk(), file=${JSON.stringify(file)}, uploadInfo=${JSON.stringify(
        uploadInfo
      )}, destinationDirectory=${JSON.stringify(destinationDirectory)}`
    );
    try {
      await execUploadFileChunk(
        destinationDirectory.key,
        file.name,
        file.size,
        uploadInfo.chunkBlob,
        uploadInfo.chunkIndex,
        uploadInfo.chunkCount,
        uploadInfo.bytesUploaded,
        uploadInfo.chunkBlob.size
      );
    } catch (e: unknown) {
      throw getFileSystemErrorFromError(e);
    }
  }

  async createDirectory(parentDirectory: FileSystemItem, name: string) {
    // => PromiseLike<any> | any
    console.log(`createDirectory(), parentDirectory=${parentDirectory.key}, name=${name}`);
    try {
      await ManageDirOrFile('', parentDirectory.key, name, Operation.CREATE_DIR);
    } catch (e: unknown) {
      throw getFileSystemErrorFromError(e);
    }
  }

  async copyItem(item: FileSystemItem, destinationDirectory: FileSystemItem) {
    // => PromiseLike<any> | any
    console.log(`copyItem(), item=${item.key}, destinationDirectory=${destinationDirectory.key}`);
    try {
      await ManageDirOrFile(item.key, destinationDirectory.key, '', Operation.COPY_FILE);
    } catch (e: unknown) {
      throw getFileSystemErrorFromError(e);
    }
  }

  async deleteItem(item: FileSystemItem) {
    // => PromiseLike<any> | any
    console.log(`deleteItem(), item=${item.key}`);
    try {
      await ManageDirOrFile(item.key, '', '', Operation.DELETE_FILE);
    } catch (e: unknown) {
      throw getFileSystemErrorFromError(e);
    }
  }

  async moveItem(item: FileSystemItem, destinationDirectory: FileSystemItem) {
    // => PromiseLike<any> | any
    console.log(`moveItem(), item=${item.key}, destinationDirectory=${destinationDirectory.key}`);
    try {
      await ManageDirOrFile(item.key, destinationDirectory.key, '', Operation.MOVE_FILE);
    } catch (e: unknown) {
      throw getFileSystemErrorFromError(e);
    }
  }

  async renameItem(item: FileSystemItem, newName: string) {
    // => PromiseLike<any> | any
    console.log(`renameItem(), item=${item.key}, newName=${newName}`);
    try {
      await ManageDirOrFile(item.key, '', newName, Operation.RENAME_FILE);
    } catch (e: unknown) {
      throw getFileSystemErrorFromError(e);
    }
  }
}

export class ServaFileSystemProvider extends CustomFileSystemProvider {
  constructor() {
    super(new ServaFileSystemOptions());
  }
}
