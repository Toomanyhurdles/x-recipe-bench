//! Developer tool: wraps a JPEG as a stand-in RAF, for trying the app
//! without a camera. The result has shooting details and a Fujifilm look,
//! but no raw data, so a real camera will refuse it.
//!
//! cargo run -p xrp-camera --example make_sample_raf -- in.jpg out.RAF [orientation]

use xrp_camera::exif;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: make_sample_raf in.jpg out.RAF [orientation 1-8]");
        std::process::exit(2);
    }
    let orientation: u16 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(1);
    let jpeg = std::fs::read(&args[1]).expect("could not read the JPEG");

    // A Classic Chrome look with a few settings changed.
    let look: Vec<(u16, u16, Vec<i64>)> = vec![
        (0x1001, 3, vec![0x82]),
        (0x1002, 3, vec![0x100]),
        (0x1003, 3, vec![0x100]),
        (0x100A, 9, vec![60, -100]),
        (0x100E, 3, vec![0x2E0]),
        (0x100F, 9, vec![0]),
        (0x1040, 9, vec![-16]),
        (0x1041, 9, vec![16]),
        (0x1047, 9, vec![32]),
        (0x104C, 3, vec![16]),
        (0x1048, 9, vec![64]),
        (0x104E, 9, vec![32]),
        (0x1401, 3, vec![0x600]),
        (0x1402, 3, vec![1]),
        (0x1403, 3, vec![200]),
    ];
    let details = exif::test_jpeg(orientation, &look);
    let mut with_details = exif::carry_over_upright(&details, &jpeg);
    // carry_over_upright marks the picture upright; put the wanted value back.
    if orientation != 1 {
        let marker = [0x12u8, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00];
        if let Some(at) = with_details.windows(marker.len()).position(|w| w == marker) {
            with_details[at + 8] = orientation as u8;
        }
    }

    let mut raf = vec![0u8; 148];
    raf[..16].copy_from_slice(b"FUJIFILMCCD-RAW ");
    raf[16..20].copy_from_slice(b"0201");
    raf[20..28].copy_from_slice(b"FF159505");
    raf[28..32].copy_from_slice(b"X-T4");
    raf[84..88].copy_from_slice(&148u32.to_be_bytes());
    raf[88..92].copy_from_slice(&(with_details.len() as u32).to_be_bytes());
    raf.extend_from_slice(&with_details);
    raf.extend_from_slice(&[0u8; 1024]);
    std::fs::write(&args[2], raf).expect("could not write the RAF");
}
