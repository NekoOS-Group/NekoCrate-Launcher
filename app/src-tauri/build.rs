fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().codegen(tauri_build::CodegenContext::new()),
    )
    .expect("cannot generate Tauri build context");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        compress_window_icon();
        replace_window_icon();
    }
}

fn compress_window_icon() {
    use std::{fs, io::BufReader, io::Write, path::PathBuf};

    const SOURCE: &str = "icons/icon.png";
    println!("cargo:rerun-if-changed={SOURCE}");
    let source = BufReader::new(fs::File::open(SOURCE).expect("window icon is missing"));
    let mut reader = png::Decoder::new(source)
        .read_info()
        .expect("window icon PNG is invalid");
    let mut rgba = vec![
        0;
        reader
            .output_buffer_size()
            .expect("window icon is too large")
    ];
    let info = reader
        .next_frame(&mut rgba)
        .expect("cannot decode window icon");
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    rgba.truncate(info.buffer_size());
    assert_eq!(
        rgba.len() as u64,
        u64::from(info.width) * u64::from(info.height) * 4
    );

    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    // Build-only reference for release checks; never included by runtime code.
    fs::write(output.join("window-icon.rgba"), &rgba).unwrap();
    // The same reversible row-difference prediction as PNG's Up filter. It
    // compresses neighboring pixels better without adding a runtime PNG decoder.
    let stride = info.width as usize * 4;
    for i in (stride..rgba.len()).rev() {
        rgba[i] = rgba[i].wrapping_sub(rgba[i - stride]);
    }
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    encoder
        .write_all(&rgba)
        .expect("cannot compress window icon");
    let compressed = encoder
        .finish()
        .expect("cannot finish window icon compression");
    fs::write(output.join("window-icon.zlib"), compressed).unwrap();
    fs::write(
        output.join("window-icon.rs"),
        format!(
            "const WIDTH: u32 = {};\nconst HEIGHT: u32 = {};\n",
            info.width, info.height
        ),
    )
    .unwrap();
}

// Tauri creates Context on a separate thread, so replacing its icon afterwards
// leaves the original uncompressed image in the binary. Use its supported build
// codegen path, replacing only the constructor's icon argument before compiling.
// An upstream constructor change must fail the build, never silently ship two icons.
fn replace_window_icon() {
    use quote::ToTokens;
    use syn::visit_mut::{self, VisitMut};

    struct ReplaceIcon(usize);
    impl VisitMut for ReplaceIcon {
        fn visit_expr_call_mut(&mut self, call: &mut syn::ExprCall) {
            if let syn::Expr::Path(function) = call.func.as_ref()
                && function
                    .path
                    .segments
                    .iter()
                    .map(|p| p.ident.to_string())
                    .eq(["tauri", "Context", "new"])
            {
                assert_eq!(call.args.len(), 8, "Tauri Context constructor changed");
                call.args[2] = syn::parse_quote!(Some(crate::window_icon::load()));
                self.0 += 1;
            }
            visit_mut::visit_expr_call_mut(self, call);
        }
    }
    let path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
        .join("tauri-build-context.rs");
    let mut context: syn::Expr = syn::parse_str(&std::fs::read_to_string(&path).unwrap())
        .expect("cannot parse generated Tauri context");
    let mut replace = ReplaceIcon(0);
    replace.visit_expr_mut(&mut context);
    assert_eq!(replace.0, 1, "Tauri Context creation changed");
    std::fs::write(path, context.into_token_stream().to_string()).unwrap();
}
