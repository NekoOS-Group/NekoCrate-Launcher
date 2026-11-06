//! Store the full-resolution Unix window icon compressed, with identical pixels.
include!(concat!(env!("OUT_DIR"), "/window-icon.rs"));

pub fn load() -> tauri::image::Image<'static> {
    let compressed = include_bytes!(concat!(env!("OUT_DIR"), "/window-icon.zlib"));
    let expected = WIDTH as usize * HEIGHT as usize * 4;
    let mut rgba = vec![0; expected];
    let mut decoder = flate2::Decompress::new(true);
    let status = decoder
        .decompress(compressed, &mut rgba, flate2::FlushDecompress::Finish)
        .expect("built-in window icon is invalid");
    assert_eq!(status, flate2::Status::StreamEnd);
    assert_eq!(decoder.total_in(), compressed.len() as u64);
    assert_eq!(decoder.total_out(), expected as u64);
    let stride = WIDTH as usize * 4;
    for i in stride..rgba.len() {
        rgba[i] = rgba[i].wrapping_add(rgba[i - stride]);
    }
    tauri::image::Image::new_owned(rgba, WIDTH, HEIGHT)
}

#[cfg(test)]
mod tests {
    #[test]
    fn context_icon_keeps_every_source_pixel_and_dimension() {
        let source = std::io::Cursor::new(include_bytes!("../icons/icon.png"));
        let mut reader = png::Decoder::new(source).read_info().unwrap();
        let mut expected = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut expected).unwrap();
        expected.truncate(info.buffer_size());
        let context = crate::application_context();
        let icon = context.default_window_icon().unwrap();
        assert_eq!((icon.width(), icon.height()), (info.width, info.height));
        assert_eq!(icon.rgba(), expected);
    }
}
