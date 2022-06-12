set windows-shell := ["pwsh.exe", "-c"]
set dotenv-load := false
echo_cmd := if os_family() == "windows" { "Write-Output" } else { "echo" }
help_prompt := if os_family() == "windows" { "\"Shell=pwsh.exe; Dir=.; Using .\\justfile\"" } else { "\"Shell=$0; Dir=`pwd`; Using ./justfile\"" }
rm_cmd := if os_family() == "windows" { "Remove-Item -Force -Recurse" } else { "rm -rf" }

help:
	@{{ echo_cmd }} {{ help_prompt }}
	@just --list

# clean generated files from webapp
clean-webapp:
	{{ rm_cmd }} webapp/build
	{{ rm_cmd }} webapp/src/generated

# clean project
clean: clean-webapp
	cargo clean

# generate grpc js to webapp source dir
gen-grpc-js:
	{{ rm_cmd }} webapp/src/generated; mkdir webapp/src/generated
	protoc -I=proto api.proto --js_out=import_style=commonjs,binary:./webapp/src/generated --grpc-web_out=import_style=typescript,mode=grpcweb:./webapp/src/generated

# build webapp only
build-webapp: gen-grpc-js
	cd webapp; yarn; yarn build

# build binary only
build-bin $SKIP_BUILD_WEBAPP="1": 
	cargo build --release

# build complete app
build: build-webapp build-bin

# run webapp within dev-server
debug-webapp:
	cd webapp; yarn start

# run binary without previous built webapp
debug-bin $RUST_LOG="serva=trace" $SKIP_BUILD_WEBAPP="1":
	cargo run

# generate release version binary and run
debug $RUST_LOG="serva=trace": build
	target/release/serva
	
count-lines:
	loc --exclude ./webapp/config --exclude ./webapp/scripts/
	tokei --exclude webapp/config --exclude webapp/scripts
