extern crate capnpc;

fn main() {
    capnpc::CompilerCommand::new()
        .output_path("src/")
        .src_prefix("schemas")
        .file("schemas/probe.capnp")
        .run()
        .expect("capnp compiles");

    capnpc::CompilerCommand::new()
        .output_path("src/")
        .src_prefix("schemas")
        .file("schemas/reply.capnp")
        .run()
        .expect("capnp compiles");
}
