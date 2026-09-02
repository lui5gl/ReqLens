use include_dir::{Dir, include_dir};

static WEB_ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/web-ui/dist");

pub fn asset(path: &str) -> Option<(&'static [u8], &'static str)> {
    let asset_path = if path == "/" {
        "index.html"
    } else {
        &path[1..]
    };
    let file = WEB_ASSETS
        .get_file(asset_path)
        .or_else(|| WEB_ASSETS.get_file("index.html"))?;
    Some((file.contents(), content_type(asset_path)))
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("woff2") => "font/woff2",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "text/html; charset=utf-8",
    }
}
