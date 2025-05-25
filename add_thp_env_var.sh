MALLOC_CONF="thp:always,metadata_thp:always" cargo build --release
mv target/release/one_brc .