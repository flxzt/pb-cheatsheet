# Configurable options
# the SDK version. Either "5.19", "6.5", "6.8"
pb_sdk_version := "6.8"
pb_device := "PB743G"
gdbserver_port := "10003"

# Either 'true' or 'false'
ci := "false"
sudo_cmd := if ci == "true" {
    ""
} else {
    "sudo"
}
linux_distr := `lsb_release -ds | tr '[:upper:]' '[:lower:]'`
cargo_sdk_feature := "sdk-" + replace(pb_sdk_version, ".", "-")
rust_backtrace := "1"
rust_loglevel := "debug"
cargo_profile := "dev"
client_build_target := "armv7-unknown-linux-gnueabi"
zigbuild_target := "armv7-unknown-linux-gnueabi.2.23"
cargo_out_profile := if cargo_profile == "dev" { "debug" } else { cargo_profile }
host_service_name := "pb-cheatsheet-host.service"

default:
    just --list

prerequisites:
    #!/usr/bin/env bash
    if [[ ('{{linux_distr}}' =~ 'fedora') || ('{{linux_distr}}' =~ 'centos') || ('{{linux_distr}}' =~ 'rhel') ]]; then
        {{sudo_cmd}} dnf install -y zig protoc
    elif [[ '{{linux_distr}}' =~ 'debian' || '{{linux_distr}}' =~ 'ubuntu' ]]; then
        {{sudo_cmd}} apt-get update
        {{sudo_cmd}} apt-get install -y zig protoc
    else
        echo "Can't install system dependencies, unsupported distro."
        exit 1
    fi
    rustup target add {{client_build_target}}
    cargo install cargo-zigbuild

fmt *ARGS:
    cargo fmt

lint *ARGS:
    cargo clippy --features={{cargo_sdk_feature}} -- {{ARGS}}

build-host:
    cargo build --profile {{cargo_profile}} -p pb-cheatsheet-host

install-host: build-host
    cargo install --profile {{cargo_profile}} --path ./crates/host

run-host *ARGS: build-host
    cargo run --profile {{cargo_profile}} -p pb-cheatsheet-host -- {{ARGS}}

deploy-host-service pb_grpc_addr="51151": install-host
    #!/usr/bin/env bash
    set -euxo pipefail

    mkdir -p $HOME/.local/share/systemd/user
    systemctl --user disable --now {{host_service_name}} || true

    cat << EOF > $HOME/.local/share/systemd/user/{{host_service_name}}
    [Unit]
    Description=pb-cheatsheet-host focused window reporter
    StartLimitIntervalSec=0
    StartLimitBurst=0

    [Service]
    Environment="PB_GRPC_ADDR={{pb_grpc_addr}}"
    Environment="RUST_LOG=pb_cheatsheet_host={{rust_loglevel}}"
    ExecStart=%h/.cargo/bin/pb-cheatsheet-host report-focused-window
    Restart=on-failure
    RestartSec=30

    [Install]
    WantedBy=default.target
    EOF

    systemctl --user enable --now {{host_service_name}}

build-testclient:
    cargo build --profile {{cargo_profile}} -p pb-cheatsheet-testclient

run-testclient: build-testclient
    cargo run --profile {{cargo_profile}} -p pb-cheatsheet-testclient

build-pb-client:
    cargo zigbuild --target {{zigbuild_target}} --profile {{cargo_profile}} -p pb-cheatsheet-client --features={{cargo_sdk_feature}}

# Transfer a built app to the device over USB. 'path_to_binary' is a relative path from 'target/<client_build_target>/<cargo_out_profile>/'.
transfer-pb-client-usb: build-pb-client
    cp {{ "target" / client_build_target / cargo_out_profile / "pb-cheatsheet-client" }} {{"/run/media/$USER" / pb_device / "applications" / "pb-cheatsheet.app" }}
    sync

# Launch `app-receiver.app` first on the device. Uses `utils/app-sender.sh` to send the application.
transfer-pb-client-remote path_to_binary remote_app_name remote_ip remote_port="19991": build-pb-client
    echo "Sending application '{{path_to_binary}}' .."
    ./utils/app-sender.sh {{ "target" / client_build_target / cargo_out_profile / path_to_binary}} {{remote_app_name}} {{remote_ip}} {{remote_port}}
    echo "Sending application was successful!"

generate-cheatsheet in_typst_path out_png_path:
    typst compile -f png {{in_typst_path}} {{out_png_path}}
    convert {{out_png_path}} -rotate -90 {{out_png_path}}

[confirm]
clean:
    cargo clean
