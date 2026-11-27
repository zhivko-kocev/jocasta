use super::*;
use crate::shared::testing::temp_jocasta;

#[test]
fn a_screenshot_is_known_by_its_first_bytes_0706() {
	assert_eq!(image_kind(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
	assert_eq!(image_kind(&[0xff, 0xd8, 0xff, 0xe0]), Some("jpg"));
	assert_eq!(image_kind(b"GIF89a"), Some("gif"));
	assert_eq!(image_kind(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
	assert_eq!(image_kind(b"-----BEGIN OPENSSH PRIVATE KEY-----"), None);
}

#[test]
fn only_real_images_are_copied_into_the_briefs_assets_and_the_rest_are_named_0706() {
	let temp = temp_jocasta();
	let base = temp.root().join("wt");
	std::fs::create_dir_all(&base).unwrap();
	std::fs::write(base.join("home.png"), b"\x89PNG\r\n\x1a\npixels").unwrap();
	std::fs::write(base.join("id_rsa"), b"-----BEGIN OPENSSH PRIVATE KEY-----").unwrap();
	let given = ["home.png".to_owned(), "id_rsa".to_owned(), "missing.png".to_owned()];
	let (names, refused) = copy_shots(&given, &base, "thread-t1", 2);
	assert_eq!(names, ["2-1.png"]);
	assert_eq!(refused.len(), 2);
	assert!(refused[0].starts_with("screenshot id_rsa skipped"));
	let kept = brief_dir().join("assets").join("thread-t1");
	assert_eq!(std::fs::read(kept.join("2-1.png")).unwrap(), b"\x89PNG\r\n\x1a\npixels");
	assert!(!kept.join("2-2.png").exists() && !kept.join("2-2").exists());
	assert_eq!(copy_shots(&given, &base, "../escape", 1), (Vec::new(), Vec::new()), "a bad brief id copies nothing");
}
