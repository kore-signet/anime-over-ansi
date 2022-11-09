#[cfg(feature = "cuda")]
use cuda_builder::CudaBuilder;

fn main() {
    #[cfg(feature = "cuda")]
    CudaBuilder::new("../gpu")
        .copy_to("cuda.ptx")
        .build()
        .unwrap();
}
