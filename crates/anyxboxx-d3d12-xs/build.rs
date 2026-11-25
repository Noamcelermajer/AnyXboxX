//! First step of D3D12.X: the functions whose contract is the same on PC are forwarded to
//! Windows' d3d12.dll by the linker, no code (the four root-signature serialisation functions,
//! which only exchange public structures and ID3DBlob blobs). Same list as
//! `runtime/d3d12_xs/d3d12_xs.cpp`.

const FORWARDED: [&str; 4] = [
    "D3D12SerializeRootSignature",
    "D3D12SerializeVersionedRootSignature",
    "D3D12CreateRootSignatureDeserializer",
    "D3D12CreateVersionedRootSignatureDeserializer",
];

fn main() {
    let forwarders: Vec<(String, String)> = FORWARDED
        .iter()
        .map(|name| ((*name).to_string(), format!("d3d12.{name}")))
        .collect();
    // The one function this crate implements (src/lib.rs).
    anyxboxx_forwarders::link("d3d12_xs", &forwarders, &["D3D12ConfigureVirtualMemoryX"]);
}
