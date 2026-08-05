# justfile pb-cheatsheet

# If invoked in CI. Either 'true' or 'false'
ci := "false"
# Cargo build profile.
cargo_profile := "dev"
# Pocketbook device type
# - Pocketbook Inkpad 4: "6678-3C5A"
# - Pocketbook Touch Lux 3: "PB626"
pb_device := "6678-3C5A"
# Pocketbook libc version
# - Pocketbook Inkpad 4: "2.39"
# - Pocketbook Touch Lux 3: "2.23"
pb_libc_version := "2.39"
# Pocketbook SDK version. Either "5.19", "6.5", "6.8"
pb_sdk_version := "5.19"
# Build target triple for Pocketbook device
pb_build_target := "armv7-unknown-linux-gnueabi"
# Pocketbook device IP.
pb_ip := "192.168.0.61"
# Pocketbook SSH host
pb_ssh_host := "pb-inkpad4-koreader"

[private]
sudo_cmd := if ci == "true" { "" } else { "sudo" }
[private]
linux_distr := `grep -o -E '^ID=([a-zA-Z0-9_]*)$' -r /etc/os-release | cut -d= -f2 | tr '[:upper:]' '[:lower:]'`
[private]
client_build_target := pb_build_target
[private]
client_zigbuild_target := pb_build_target + "." + pb_libc_version
[private]
cargo_out_profile := if cargo_profile == "dev" { "debug" } else { cargo_profile }
[private]
cargo_sdk_feature := "sdk-" + replace(pb_sdk_version, ".", "-")
[private]
host_service_name := "pb-cheatsheet-host.service"

export RUST_LOG := env("RUST_LOG", "debug")
export RUST_BACKTRACE := env("RUST_BACKTRACE", "1")

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
    cargo install --locked cargo-zigbuild
    cargo install --locked typst-cli

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

deploy-host-service: install-host
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
    Environment="PB_CHEATSHEET_RPC_ADDR={{pb_ip}}:50051"
    Environment="RUST_LOG=pb_cheatsheet_host=info"
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

build-client:
    cargo zigbuild \
        --target {{client_zigbuild_target}} \
        --profile {{cargo_profile}} \
        -p pb-cheatsheet-client \
        --no-default-features \
        --features={{cargo_sdk_feature}}

deploy-client-usb: build-client
    cp {{ "target" / client_build_target / cargo_out_profile / "pb-cheatsheet-client" }} \
        {{"/run/media/$USER" / pb_device / "applications" / "pb-cheatsheet.app" }}
    sync

[doc('Deploy the client application to the device over SSH.
Make sure a SSH connection is available.')]
deploy-client-ssh: build-client
    scp {{ "target" / client_build_target / cargo_out_profile / "pb-cheatsheet-client"}} \
        {{pb_ssh_host}}:/mnt/ext1/applications/pb-cheatsheet.app

[doc('Launch a GDB server session on the device.
Make sure a SSH connection is available.')]
launch-gdbserver:
    ssh {{pb_ssh_host}} gdbserver 0.0.0.0:10003 /mnt/ext1/applications/pb-cheatsheet.app 

generate-cheatsheets:
    #!/usr/bin/env bash
    set -euxo pipefail
    for in_file in ./cheatsheets/*.typ; do
        out_file="${in_file%.typ}.png"
        typst compile -f png "${in_file}" "${out_file}"
        magick "${out_file}" -rotate -90 "${out_file}"
    done
