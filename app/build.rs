fn main() {
    // `linkall.x` must be the last linker script so the ESP HAL's sections
    // are retained correctly.
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}
