MALLOC_CONF="thp:always,metadata_thp:always" cargo build --release --target x86_64-unknown-linux-gnu
mv target/x86_64-unknown-linux-gnu/release/one_brc .