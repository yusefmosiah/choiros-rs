// BAML Generator for Rust - ChoirOS Chat Agent
// This generates Rust code for BAML functions

generator target {
    // Valid values: "python/pydantic", "typescript", "go", "rust", "ruby/sorbet", "rest/openapi"
    output_type "rust"

    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../sandbox/src"

    // The version of the BAML package you have installed
    version "0.217.0"

    // Valid values: "sync", "async"
    // This controls what `b.FunctionName()` will be (sync or async).
    default_client_mode async
}
