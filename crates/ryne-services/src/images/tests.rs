use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::time::Duration;

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="#ff0000" fill-opacity="0.5"/></svg>"##;
fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([25, 50, 100, 128]));
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, ImageFormat::Png).unwrap();
    output.into_inner()
}
fn data(bytes: &[u8], base64: bool, mime: &str) -> String {
    if base64 {
        format!("data:{mime};base64,{}", STANDARD.encode(bytes))
    } else {
        format!(
            "data:{mime},{}",
            percent_encoding::percent_encode(bytes, percent_encoding::NON_ALPHANUMERIC)
        )
    }
}
#[test]
fn decodes_pixels_and_preserves_alpha() {
    let resource = decode(&png(2, 3), &CancelControl::default()).unwrap();
    assert_eq!((resource.pixel_width, resource.pixel_height), (2, 3));
    assert_eq!(&resource.pixels[..4], &[100, 50, 25, 128]);
    let svg = decode(SVG.as_bytes(), &CancelControl::default()).unwrap();
    assert_eq!((svg.width, svg.height), (20., 10.));
    assert_eq!((svg.pixel_width, svg.pixel_height), (40, 20));
    assert_eq!(&svg.pixels[..4], &[0, 0, 255, 128]);
}
#[test]
fn file_and_data_urls_support_raster_svg_and_encoded_names() {
    let directory = tempfile::tempdir().unwrap();
    for (name, bytes, mime) in [
        ("图片 # % 100.png", png(2, 3), "image/png"),
        ("示例 # % 100.svg", SVG.as_bytes().to_vec(), "image/svg+xml"),
    ] {
        let path = directory.path().join(name);
        std::fs::write(&path, &bytes).unwrap();
        let file = url::Url::from_file_path(path).unwrap();
        for address in [
            file.to_string(),
            data(&bytes, true, mime),
            data(&bytes, false, mime),
        ] {
            let result = ImageRenderer
                .render(&address, CancelControl::default())
                .unwrap();
            assert!(result.width > 0. && result.bytes > 0);
        }
    }
    // Header case, charset, an empty MIME type and ignored fragments are valid.
    for address in [
        data(&png(1, 1), true, ""),
        data(SVG.as_bytes(), false, "image/svg+xml;charset=UTF-8").replacen("data:", "DATA:", 1),
        format!("{}#fragment", data(&png(1, 1), true, "image/png")),
    ] {
        assert!(ImageRenderer
            .render(&address, CancelControl::default())
            .is_ok());
    }
}
#[test]
fn data_larger_than_old_url_limit_decodes_but_expansion_is_bounded() {
    let svg = SVG.replace("</svg>", &format!("<!--{}--></svg>", "x".repeat(9000)));
    for base64 in [true, false] {
        let address = data(svg.as_bytes(), base64, "image/svg+xml");
        assert!(address.len() > 8192);
        assert!(ImageRenderer
            .render(&address, CancelControl::default())
            .is_ok());
    }
    let oversized = data(&vec![0; MAX_DOWNLOAD + 1], true, "image/png");
    assert!(ImageRenderer
        .render(
            &oversized,
            CancelControl::with_timeout(Duration::from_secs(10))
        )
        .unwrap_err()
        .contains("8 MiB"));
    let huge_header = format!("data:{};base64,AA==", "x".repeat(8193));
    assert!(ImageRenderer
        .render(&huge_header, CancelControl::default())
        .is_err());
}
#[test]
fn invalid_oversized_and_cancelled_sources_fail_without_partial_images() {
    for bytes in [
        b"not an image".to_vec(),
        b"<svg".to_vec(),
        vec![0; MAX_DOWNLOAD + 1],
        png(4097, 1),
    ] {
        assert!(decode(&bytes, &CancelControl::default()).is_err());
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("huge.png");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_DOWNLOAD as u64 + 1)
        .unwrap();
    for source in [
        url::Url::from_file_path(path).unwrap().to_string(),
        url::Url::from_file_path(directory.path())
            .unwrap()
            .to_string(),
        url::Url::from_file_path(directory.path().join("missing.png"))
            .unwrap()
            .to_string(),
        "data:image/png;base64,AA==".into(),
        "data:image/png;base64,!!".into(),
        "data:invalid".into(),
        "relative.png".into(),
        "https://user:password@example.org/a.png".into(),
        "ftp://example.org/image%0D%0ADELE%20file.png".into(),
    ] {
        assert!(ImageRenderer
            .render(&source, CancelControl::default())
            .is_err());
    }
    let control = CancelControl::default();
    control.cancel();
    assert!(ImageRenderer
        .render(&data(SVG.as_bytes(), true, "image/svg+xml"), control)
        .is_err());
}
#[test]
fn svg_resources_are_self_contained_and_complexity_is_bounded() {
    let embedded = format!(
        r#"<image href="{}" width="20" height="10"/>"#,
        data(&png(2, 3), true, "image/png")
    );
    let good = SVG.replace("</svg>", &format!("{embedded}</svg>"));
    assert!(decode(good.as_bytes(), &CancelControl::default()).is_ok());
    for element in [
        r#"<image href="https://example.org/image.png" width="20" height="10"/>"#,
        r#"<image href="/tmp/image.png" width="20" height="10"/>"#,
        r#"<use href="file:///tmp/image.svg#shape"/>"#,
    ] {
        let svg = SVG.replace("</svg>", &format!("{element}</svg>"));
        assert!(decode(svg.as_bytes(), &CancelControl::default()).is_err());
    }
    let deep = SVG.replace(
        "</svg>",
        &format!("{}{}{}", "<g>".repeat(65), "</g>".repeat(65), "</svg>"),
    );
    assert!(decode(deep.as_bytes(), &CancelControl::default()).is_err());
    assert!(decode(
        SVG.replace("width=\"20\"", "width=\"5000\"").as_bytes(),
        &CancelControl::default()
    )
    .is_err());
    assert!(decode(
        include_bytes!("../../../../assets/icons/image_failed.svg"),
        &CancelControl::default()
    )
    .is_ok());
}
#[test]
fn large_images_are_downsampled_without_changing_the_layout_aspect_ratio() {
    let resource = decode(
        &png(1680, 1576),
        &CancelControl::with_timeout(Duration::from_secs(10)),
    )
    .unwrap();
    assert_eq!((resource.width, resource.height), (1680., 1576.));
    assert!(u64::from(resource.pixel_width) * u64::from(resource.pixel_height) <= MAX_PIXELS);
    assert!(resource.pixel_width < 1680);
}
#[test]
#[ignore = "explicit external network integration check"]
fn user_example_downloads_and_decodes() {
    let image = ImageRenderer
        .render(
            "https://gw.alipayobjects.com/zos/k/t0/43.png",
            CancelControl::default(),
        )
        .unwrap();
    assert!(image.width > 1. && image.height > 1.);
}

mod network;
