# Serva - A simple http server for transfer files between devices

## Features

* No dependencies - comes as a single executable
* More options - allow user manage and upload files on server
* Cross platform - could work on windows and linux

## Build prerequisite

* yarn
* protobuf-compiler
* [protoc-gen-grpc-web](https://github.com/grpc/grpc-web/releases)
* just (Optional)
* grpcurl (Optional)

## Build

Refer to [justfile](./justfile) for build commands.

Or just simply use: `cargo build --release`