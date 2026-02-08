# justfile pb-cheatsheet

# Configurable options
# the SDK version. Either "5.19", "6.5", "6.8"
pb_sdk_version := "6.8"
# value for Pocketbook Inkpad 4
pb_device := "6678-3C5A"
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
cargo_profile := "dev"
client_build_target := "armv7-unknown-linux-gnueabi"
zigbuild_target := "armv7-unknown-linux-gnueabi.2.23"
cargo_out_profile := if cargo_profile == "dev" { "debug" } else { cargo_profile }
host_service_name := "pb-cheatsheet-host.service"

export RUST_LOG := "debug"
export RUST_BACKTRACE := "1"

default:
    just --list

[confirm]
clean:
    cargo clean

prerequisites:
    #!/usr/bin/env bash
    if [[ ('{{linux_distr}}' =~ 'fedora') ]]; then
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

deploy-host-service pb_grpc_addr="192.168.0.101:51151": install-host
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
    Environment="RUST_LOG={{RUST_LOG}}"
    Environment="RUST_BACKTRACE={{RUST_BACKTRACE}}"
    ExecStart=%h/.cargo/bin/pb-cheatsheet-host report-focused-window
    Restart=on-failure
    RestartSec=5m

    [Install]
    WantedBy=default.target
    EOF

    systemctl --user enable --now {{host_service_name}}

build-testclient:
    cargo build --profile {{cargo_profile}} -p pb-cheatsheet-testclient

run-testclient: build-testclient
    cargo run --profile {{cargo_profile}} -p pb-cheatsheet-testclient

build-pb-client:
    cargo zigbuild \
        --target {{zigbuild_target}} \
        --profile {{cargo_profile}} \
        -p pb-cheatsheet-client \
        --no-default-features \
        --features={{cargo_sdk_feature}}

[doc('Transfer a built app to the device over USB.
"path_to_binary" is a relative path from "target/<client_build_target>/<cargo_out_profile>/".')]
transfer-pb-client-usb: build-pb-client
    cp {{ "target" / client_build_target / cargo_out_profile / "pb-cheatsheet-client" }} \
        {{"/run/media/$USER" / pb_device / "applications" / "pb-cheatsheet.app" }}
    sync

transfer-app-receiver:
    cp {{ "utils" / "app-receiver.app" }} \
        {{"/run/media/$USER" / pb_device / "applications" / "app-receiver.app" }}
    sync

[doc('Launch `app-receiver.app` first on the device.
"path_to_binary" is a relative path from "target/<client_build_target>/<cargo_out_profile>/".
Uses `utils/app-sender.sh` to send the application.')]
transfer-pb-client-remote path_to_binary remote_app_name remote_ip remote_port="19991": build-pb-client
    echo "Sending application '{{path_to_binary}}' .."
    ./utils/app-sender.sh \
        {{ "target" / client_build_target / cargo_out_profile / path_to_binary}} \
        {{remote_app_name}} \
        {{remote_ip}} \
        {{remote_port}}
    echo "Sending application was successful!"

generate-cheatsheets:
    #!/usr/bin/env bash
    set -euxo pipefail
    for in_file in ./cheatsheets/*.typ; do
        out_file="${in_file%.typ}.png"
        typst compile -f png "${in_file}" "${out_file}"
        convert "${out_file}" -rotate -90 "${out_file}"
    done
