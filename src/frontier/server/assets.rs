//! The canvas page's files by URL path (`/` is the HTML). The Rust binary carries the bundle Bun built ahead of time
//! (0639): `build.rs` bundles `src/frontier/canvas/index.html` with the options `assets.ts` passes to `Bun.build` (minify,
//! browser target, splitting) and generates the table [`PAGE_FILES`] this module serves from, so no working directory or
//! install folder matters (0367).

use std::sync::LazyLock;

use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::Response;
use regex::Regex;

include!(concat!(env!("OUT_DIR"), "/assets_table.rs"));

/// A file name Bun content-hashed (`index-37eqm9x7.js`): its bytes never change under that name.
static HASHED: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"-[A-Za-z0-9_-]{8}\.(js|css|woff2|ttf|wasm)$").expect("the hashed-name pattern"));

/// How long the browser may keep a page file (PM-3): a hashed file for a year, so it isn't fetched and compiled again
/// for every review; the HTML and anything unhashed never, so a new build shows at once.
pub fn asset_cache_control(pathname: &str) -> &'static str {
	if HASHED.is_match(pathname) { "public, max-age=31536000, immutable" } else { "no-store" }
}

/// The page file at `pathname` with its content type, or `None` when there is none.
pub fn page_file(pathname: &str) -> Option<(&'static str, &'static [u8])> {
	let pathname = if pathname == "/index.html" { "/" } else { pathname };
	PAGE_FILES.iter().find(|(url, _, _)| *url == pathname).map(|(_, content_type, body)| (*content_type, *body))
}

/// The page file at `pathname`, or `None` when there is none.
pub async fn ui_asset(pathname: &str) -> Option<Response> {
	let (content_type, body) = page_file(pathname)?;
	Response::builder()
		.status(StatusCode::OK)
		.header(header::CONTENT_TYPE, content_type)
		.header(header::CACHE_CONTROL, asset_cache_control(pathname))
		.body(Body::from(body))
		.ok()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn hashed_page_files_are_kept_a_year_the_html_and_unhashed_files_never_pm_3() {
		assert_eq!(asset_cache_control("/index-37eqm9x7.js"), "public, max-age=31536000, immutable");
		assert_eq!(asset_cache_control("/MonaSans-VF-d0xc4p05.woff2"), "public, max-age=31536000, immutable");
		assert_eq!(asset_cache_control("/"), "no-store");
		assert_eq!(asset_cache_control("/ptt-worker.js"), "no-store");
	}

	/// The TypeScript test bundles a server and runs it from another working directory; the Rust binary embeds the
	/// page, so what the test keeps is that the page and its script are served with no file system under them (0367).
	#[tokio::test]
	async fn the_embedded_page_serves_its_html_and_the_script_it_names() {
		let page = ui_asset("/").await.expect("the page");
		assert_eq!(page.status(), StatusCode::OK);
		assert_eq!(page.headers()[header::CONTENT_TYPE], "text/html;charset=utf-8");
		assert_eq!(page.headers()[header::CACHE_CONTROL], "no-store");
		let html =
			String::from_utf8(axum::body::to_bytes(page.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
		assert!(html.contains("<title>Jocasta</title>"));
		// The page names its script relative to the HTML's place in the bundle; the browser resolves it to the root.
		let script = Regex::new(r#"<script[^>]*src="[^"]*?([^"/]+\.js)""#).unwrap().captures(&html).expect("a script")
			[1]
		.to_owned();
		let chunk = ui_asset(&format!("/{script}")).await.expect("the script");
		assert_eq!(chunk.status(), StatusCode::OK);
		assert_eq!(chunk.headers()[header::CONTENT_TYPE], "text/javascript;charset=utf-8");
		assert_eq!(chunk.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
	}

	#[tokio::test]
	async fn the_index_html_path_is_the_page_and_an_unknown_path_has_no_file() {
		let index = ui_asset("/index.html").await.expect("the page");
		let root = ui_asset("/").await.expect("the page");
		assert_eq!(index.headers()[header::CACHE_CONTROL], "no-store");
		assert_eq!(index.headers()[header::CONTENT_TYPE], root.headers()[header::CONTENT_TYPE]);
		assert!(ui_asset("/nope.js").await.is_none());
		assert!(ui_asset("/api/state").await.is_none());
	}

	#[test]
	fn the_bundle_carries_the_fonts_and_monacos_worker() {
		assert!(
			PAGE_FILES
				.iter()
				.any(|(url, content_type, _)| url.starts_with("/MonaSans-VF-") && *content_type == "font/woff2")
		);
		assert!(
			PAGE_FILES.iter().any(|(url, content_type, _)| url.starts_with("/codicon-") && *content_type == "font/ttf")
		);
		assert!(!MONACO_WORKER.is_empty());
	}
}
