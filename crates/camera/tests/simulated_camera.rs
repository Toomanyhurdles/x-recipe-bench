//! Runs the real protocol code against a simulated camera, so the framing
//! and the conversion sequence are checked without hardware.

use xrp_camera::error::Error;
use xrp_camera::ptp;
use xrp_camera::sim::{FakeCamera, FAKE_JPEG};
use xrp_camera::{slots, Camera, RenderSize, UsbMode};

fn fake_raf(len: usize) -> Vec<u8> {
    let mut raf = b"FUJIFILMCCD-RAW 0201FF159505X-T4".to_vec();
    raf.resize(len, 0xAB);
    raf
}

#[test]
fn connects_and_reads_details() {
    let cam = Camera::start(FakeCamera::new()).unwrap();
    assert_eq!(cam.info.model, "X-T4");
    assert_eq!(cam.info.firmware, "2.12");
    assert!(cam.info.can_convert());
    assert_eq!(cam.mode, UsbMode::RawConversion);
    cam.require_conversion_mode().unwrap();
}

#[test]
fn develops_a_raf_end_to_end() {
    let mut cam = Camera::start(FakeCamera::new()).unwrap();
    cam.upload_raf(&fake_raf(5000)).unwrap();
    assert_eq!(cam.read_profile().unwrap().len(), 621);
    let jpeg = cam.develop(RenderSize::Full).unwrap();
    assert_eq!(jpeg, FAKE_JPEG);
    // A second conversion needs no second upload.
    let jpeg = cam.develop(RenderSize::Preview).unwrap();
    assert_eq!(jpeg, FAKE_JPEG);
}

#[test]
fn survives_a_response_glued_to_the_data() {
    let mut fake = FakeCamera::new();
    fake.glue_response = true;
    let mut cam = Camera::start(fake).unwrap();
    cam.upload_raf(&fake_raf(4096)).unwrap();
    assert_eq!(cam.develop(RenderSize::Full).unwrap(), FAKE_JPEG);
}

#[test]
fn rejects_a_file_that_is_not_a_raf() {
    let mut cam = Camera::start(FakeCamera::new()).unwrap();
    assert!(matches!(cam.upload_raf(b"not a raf at all"), Err(Error::BadRaf(_))));
}

#[test]
fn explains_a_raf_from_another_body() {
    let mut fake = FakeCamera::new();
    fake.foreign_raf = true;
    let mut cam = Camera::start(fake).unwrap();
    cam.upload_raf(&fake_raf(2000)).unwrap();
    let err = cam.read_profile().unwrap_err();
    assert!(err.to_string().contains("this exact camera body"), "{err}");
}

#[test]
fn reports_the_wrong_usb_mode() {
    let mut fake = FakeCamera::new();
    fake.mode = 5;
    let cam = Camera::start(fake).unwrap();
    assert_eq!(cam.mode, UsbMode::Tether);
    assert!(matches!(cam.require_conversion_mode(), Err(Error::WrongMode(_))));
}

#[test]
fn object_info_round_trips() {
    let d = ptp::encode_object_info(0xF802, 59_044_160, "FUP_FILE.dat");
    assert_eq!(d.len(), 52 + 27 + 3);
    let info = ptp::decode_object_info(&d).unwrap();
    assert_eq!((info.format, info.size, info.filename.as_str()), (0xF802, 59_044_160, "FUP_FILE.dat"));
}

#[test]
fn raf_header_is_parsed() {
    let mut h = vec![0u8; 92];
    h[..16].copy_from_slice(b"FUJIFILMCCD-RAW ");
    h[28..32].copy_from_slice(b"X-T4");
    h[84..88].copy_from_slice(&148u32.to_be_bytes());
    h[88..92].copy_from_slice(&1000u32.to_be_bytes());
    let parsed = xrp_camera::raf::parse_header(&h).unwrap();
    assert_eq!(parsed.model, "X-T4");
    assert_eq!((parsed.jpeg_offset, parsed.jpeg_len), (148, 1000));
}

#[test]
fn a_changed_profile_is_written_read_back_and_developed() {
    use xrp_camera::profile::{cell, Profile};
    let mut cam = Camera::start(FakeCamera::new()).unwrap();
    cam.upload_raf(&fake_raf(3000)).unwrap();
    let as_shot = Profile::from_bytes(cam.read_profile().unwrap()).unwrap();
    assert_eq!(as_shot.cell_count(), 28);

    let mut changed = as_shot.clone();
    changed.set(cell::FILM_SIMULATION, 0x0B);
    cam.write_profile(&changed.to_host_write()).unwrap();

    let now = Profile::from_bytes(cam.read_profile().unwrap()).unwrap();
    assert_eq!(now.get(cell::FILM_SIMULATION), Some(0x0B));
    assert!(changed.differences(&now).is_empty());
    assert_eq!(cam.develop(RenderSize::Preview).unwrap(), FAKE_JPEG);
}

#[test]
fn reads_every_custom_slot_and_leaves_the_pointer_where_it_was() {
    let mut cam = Camera::start(FakeCamera::with_slots()).unwrap();
    let slots = cam.read_slots().unwrap();
    assert_eq!(slots.len(), 7, "counted by trying until the camera says no");
    assert_eq!(slots[2].number, 3);
    assert_eq!(slots[2].name, "CUSTOM 3");
    assert_eq!(slots[2].get(slots::prop::FILM_SIMULATION), Some(1));
    assert_eq!(slots[2].values.len(), 23);
    // Reading must not leave the camera pointing at another slot.
    assert_eq!(cam.read_slot(3).unwrap().number, 3);
}

#[test]
fn writes_a_slot_and_reports_what_the_camera_did_with_each_setting() {
    use slots::prop::*;
    let mut cam = Camera::start(FakeCamera::with_slots()).unwrap();
    let writes = [
        (FILM_SIMULATION, 0x0B),   // Classic Chrome
        (MONO_WARM_COOL, 20),      // refused: not a black-and-white simulation
        (GRAIN, 1),                // off: accepted, stored as 7
        (WHITE_BALANCE, 0x8007),
        (COLOR_TEMPERATURE, 6300), // accepted because the mode is now Kelvin
        (HIGHLIGHT, (-10i16) as u16),
    ];
    let result = cam.write_slot(5, Some("Campfire"), &writes).unwrap();
    assert_eq!(result.name_response, Some(0x2001));
    assert_eq!(result.name_after, "Campfire");
    let outcome = |prop: u16| result.outcomes.iter().find(|o| o.prop == prop).unwrap().clone();
    assert_eq!((outcome(FILM_SIMULATION).response, outcome(FILM_SIMULATION).read_back), (0x2001, Some(0x0B)));
    assert_eq!((outcome(MONO_WARM_COOL).response, outcome(MONO_WARM_COOL).read_back), (0x201C, Some(0)));
    assert_eq!((outcome(GRAIN).response, outcome(GRAIN).read_back), (0x2001, Some(7)));
    assert_eq!(outcome(COLOR_TEMPERATURE).read_back, Some(6300));
    assert_eq!(outcome(HIGHLIGHT).read_back, Some((-10i16) as u16));

    // Only slot 5 changed.
    let slots = cam.read_slots().unwrap();
    assert_eq!(slots[4].name, "Campfire");
    assert_eq!(slots[3].name, "CUSTOM 4");
    assert_eq!(slots[3].get(FILM_SIMULATION), Some(1));
    assert!(cam.write_slot(8, None, &[]).is_err(), "there is no C8");
}

#[test]
fn a_body_without_slot_commands_says_so() {
    let mut cam = Camera::start(FakeCamera::new()).unwrap();
    let problem = cam.read_slots().unwrap_err().to_string();
    assert!(problem.contains("custom-slot"), "{problem}");
}
