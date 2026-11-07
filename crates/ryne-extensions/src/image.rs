//! Shared input bounds for image providers and their rendering services.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_IMAGE_URL: usize = 8192;
// Percent encoding can expand every input byte into three ASCII bytes.
pub const MAX_DATA_IMAGE_URL: usize = MAX_IMAGE_BYTES * 3 + MAX_IMAGE_URL;

pub fn is_data_url(source: &str) -> bool {
    source
        .get(..5)
        .is_some_and(|s| s.eq_ignore_ascii_case("data:"))
}

pub fn validate_image_url(source: &str) -> Result<(), String> {
    if is_data_url(source) {
        if source.len() > MAX_DATA_IMAGE_URL {
            return Err("Encoded image exceeds 24 MiB".into());
        }
        if !source
            .find(',')
            .is_some_and(|header| header <= MAX_IMAGE_URL)
        {
            return Err("Invalid or oversized data URL header".into());
        }
    } else if source.len() > MAX_IMAGE_URL {
        return Err("Image URL exceeds 8 KiB".into());
    }
    Ok(())
}
