cargo build --release

rm -rf target/release/assets

cp -r assets target/release/
