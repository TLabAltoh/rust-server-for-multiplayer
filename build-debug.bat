cargo build

rm -rf target/debug/assets/webui

cp -r assets/webui target/debug/
