use ipfs_cid::consts::{CODEC_RAW, SHA2_256};
use ipfs_cid::models::Cid;

fn main() {
    let data = b"Hello xWorld!";

    // Generate the CID
    let cid = Cid::generate_from_data(data, CODEC_RAW, SHA2_256).expect("CID generation failed");

    // Print to the console
    println!("My generated CID is: {}", cid.to_string_base32());
}
