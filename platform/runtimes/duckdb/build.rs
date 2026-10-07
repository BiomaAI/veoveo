#[cfg(feature = "smoke")]
#[path = "src/staging.rs"]
mod staging;
fn main() {
    #[cfg(feature = "smoke")]
    {
        let _ = std::any::TypeId::of::<duckdb::Connection>();
        staging::stage_runtime();
    }
}
