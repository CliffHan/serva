import FileManager, { Permissions } from "devextreme-react/file-manager";
import React, { createRef } from "react";
import "./App.css";
import { DownloadState, EventBus, EventContextDownload, EVENT_DOWNLOAD, ServaFileSystemProvider } from "./FileSystem";
import { Config, GetConfig, Permission } from "./GrpcClient";
// import { Popup } from 'devextreme-react/popup';

const IS_DEVELOPMENT = process.env.NODE_ENV === "development";

const fileSystemProvider = new ServaFileSystemProvider();
const DEFAULT_PERMISSION: Permission = {
  create: false,
  copy: false,
  move: false,
  delete: false,
  rename: false,
  upload: false,
  download: false,
};

declare global {
  interface Window {
    app: any;
    filemanager: any;
    eventbus: any;
  }
}

type AppProps = {};
type AppState = {
  permission: Permission;
};

class App extends React.Component<AppProps, AppState> {
  config: Config | undefined;
  fileManagerRef = createRef<FileManager>();
  state: AppState = {
    permission: DEFAULT_PERMISSION,
  };
  operation: any;
  async componentDidMount() {
    //ATTENTION: debug code mixed
    if (IS_DEVELOPMENT) {
      window.app = this;
      window.filemanager = this.fileManagerRef;
      window.eventbus = EventBus;
    }
    EventBus.on(EVENT_DOWNLOAD, this.onDownloadEvent.bind(this));
    try {
      let config = await GetConfig();
      this.setState({ permission: config.permission });
      this.config = config;
    } catch (e) {
      if (e instanceof Error) {
        console.log(`failed to get config, error: ${e.message}`);
      }
    }
  }

  componentWillUnmount() {
    EventBus.removeListener(EVENT_DOWNLOAD);
  }

  onDownloadEvent(context: EventContextDownload) {
    console.log(`context=${JSON.stringify(context)}`);
    // here use some hacks to display with file manager object
    // @ts-ignore
    let filemanager = this.fileManagerRef.current._instance;
    // @ts-ignore
    let notificationControl = filemanager._notificationControl;
    let message = '';
    switch (context.state) {
      case DownloadState.STARTING:
        message = `Downloading ${context.total} file(s)`;
        this.operation = notificationControl.addOperation(message, false, false);
        break;
      case DownloadState.STEPPING:
        message = `Downloaded ${context.current}/${context.total} file(s)`;
        notificationControl.updateOperationItemProgress(this.operation, 0, 0, Math.floor(context.current*100/context.total));
        break;
      case DownloadState.STOPPING:
        message = `Downloaded ${context.current}/${context.total} file(s)`;
        notificationControl.completeOperation(this.operation, message, !context.succeeded);
        this.operation = undefined;
        if (!context.succeeded) {
          message = 'Multiple file download blocked by browser';
          filemanager._showError(message);
        }
        break;
      default:
        break;
    }
  }

  render() {
    return (
      <div className="App">
        <FileManager ref={this.fileManagerRef} fileSystemProvider={fileSystemProvider}>
          <Permissions {...this.state.permission} />
        </FileManager>
      </div>
    );
  }
}

export default App;
