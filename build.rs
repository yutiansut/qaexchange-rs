//! Build script for qaexchange-rs
//!
//! @yutiansut @quantaxis
//!
//! 编译 protobuf 定义生成 Rust 代码

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 编译 replication.proto 到 OUT_DIR (tonic::include_proto! 默认位置)
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/replication.proto"], &["proto"])?;

    // 通知 cargo 当 proto 文件变化时重新编译
    println!("cargo:rerun-if-changed=proto/replication.proto");

    Ok(())
}
