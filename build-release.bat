cargo build --release

rm -rf target/release/assets/webui

cp -r assets/webui target/release/
