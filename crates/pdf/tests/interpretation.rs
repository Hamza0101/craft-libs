#![cfg(feature = "read")]
#![allow(clippy::unwrap_used, clippy::panic)]

#[cfg(feature = "interpret")]
mod device {
    use craft_pdf::interpret::{
        BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache, InterpreterSettings, Paint, PathDrawMode, SoftMask,
        TransformExt, font::Glyph,
    };
    use kurbo::{Affine, BezPath, Rect, Shape};

    #[derive(Default)]
    struct PathDevice {
        paths: usize,
        fail: bool,
    }

    impl Device<'_> for PathDevice {
        fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
        fn set_blend_mode(&mut self, _: BlendMode) {}
        fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'_>, _: &PathDrawMode) {
            assert!(!self.fail, "synthetic device failure");
            self.paths += 1;
        }
        fn push_clip_path(&mut self, _: &ClipPath) {}
        fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
        fn draw_glyph(&mut self, _: &Glyph<'_>, _: Affine, _: Affine, _: &Paint<'_>, _: &GlyphDrawMode) {}
        fn draw_image(&mut self, _: Image<'_, '_>, _: Affine) {}
        fn pop_clip_path(&mut self) {}
        fn pop_transparency_group(&mut self) {}
    }

    fn execute(device: &mut PathDevice) -> Result<(), craft_pdf::ExecutionError> {
        let content = "1 0 0 rg 0 0 40 20 re f";
        let bytes = format!(
            "%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 40 20] /Contents 4 0 R >> endobj\n4 0 obj << /Length {} >> stream\n{content}\nendstream endobj\ntrailer << /Root 1 0 R >>\n%%EOF",
            content.len()
        );
        let pdf = craft_pdf::open_slice(bytes.as_bytes(), None).unwrap();
        let page = &pdf.pages()[0];
        let cache = InterpreterCache::new();
        let mut context = Context::new(
            page.initial_transform(true).to_kurbo(),
            Rect::new(0.0, 0.0, 40.0, 20.0),
            &cache,
            page.xref(),
            InterpreterSettings::default(),
        );
        craft_pdf::interpret_page(page, &mut context, device)
    }

    /// Records what the device is asked to paint. `draw_rect` is left at its default, which
    /// forwards to `draw_path`, so a path replaced by a rectangle is visible here as the
    /// rectangle's path.
    #[derive(Default)]
    struct RecordDevice {
        paths: Vec<BezPath>,
    }

    impl Device<'_> for RecordDevice {
        fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
        fn set_blend_mode(&mut self, _: BlendMode) {}
        fn draw_path(&mut self, path: &BezPath, _: Affine, _: &Paint<'_>, _: &PathDrawMode) {
            self.paths.push(path.clone());
        }
        fn push_clip_path(&mut self, _: &ClipPath) {}
        fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
        fn draw_glyph(&mut self, _: &Glyph<'_>, _: Affine, _: Affine, _: &Paint<'_>, _: &GlyphDrawMode) {}
        fn draw_image(&mut self, _: Image<'_, '_>, _: Affine) {}
        fn pop_clip_path(&mut self) {}
        fn pop_transparency_group(&mut self) {}
    }

    fn record(content: &str) -> Vec<BezPath> {
        let bytes = format!(
            "%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 100 60] /Contents 4 0 R >> endobj\n4 0 obj << /Length {} >> stream\n{content}\nendstream endobj\ntrailer << /Root 1 0 R >>\n%%EOF",
            content.len()
        );
        let pdf = craft_pdf::open_slice(bytes.as_bytes(), None).unwrap();
        let page = &pdf.pages()[0];
        let cache = InterpreterCache::new();
        let mut context = Context::new(
            page.initial_transform(true).to_kurbo(),
            Rect::new(0.0, 0.0, 100.0, 60.0),
            &cache,
            page.xref(),
            InterpreterSettings::default(),
        );
        let mut device = RecordDevice::default();
        craft_pdf::interpret_page(page, &mut context, &mut device).unwrap();
        device.paths
    }

    /// The points of each subpath, ignoring the closing command.
    fn points(path: &BezPath) -> Vec<(f64, f64)> {
        path.elements()
            .iter()
            .filter_map(|el| match el {
                kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => Some((p.x, p.y)),
                _ => None,
            })
            .collect()
    }

    fn closed(path: &BezPath) -> bool {
        path.elements().iter().any(|el| matches!(el, kurbo::PathEl::ClosePath))
    }

    /// A five-point open polyline is not a rectangle, even when its ends happen to touch all four
    /// corners of its own bounding box. `path_as_rect` matched it on element count alone and the
    /// whole path was painted as that box: this zigzag arrived as `40 0 m 80 0 l 80 20 l 40 20 l`
    /// closed, losing its middle peak.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn a_five_point_polyline_is_not_painted_as_its_bounding_box() {
        let zigzag = "0 0 0 RG 2 w 80 40 m 80 20 l 60 30 l 40 20 l 40 40 l S";
        let paths = record(zigzag);
        assert_eq!(paths.len(), 1);
        assert_eq!(points(&paths[0]), [(80.0, 40.0), (80.0, 20.0), (60.0, 30.0), (40.0, 20.0), (40.0, 40.0)]);
        assert!(!closed(&paths[0]), "the path was left open, as `S` leaves it");
        // Filling and filling-and-stroking took the same route.
        for op in ["f", "B"] {
            let paths = record(&format!("0 0 0 rg 0 0 0 RG 2 w 80 40 m 80 20 l 60 30 l 40 20 l 40 40 l {op}"));
            assert!(!paths.is_empty(), "{op}");
            for p in &paths {
                assert_eq!(points(p), [(80.0, 40.0), (80.0, 20.0), (60.0, 30.0), (40.0, 20.0), (40.0, 40.0)], "{op}");
            }
        }
    }

    /// A real rectangle still takes the rectangle route, written either way: `m l l l` and closed,
    /// or `m l l l l` back to its first corner. Both reach the device as four corners, closed.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn a_rectangle_is_still_recognised_however_it_is_written() {
        for content in ["1 0 0 rg 40 20 m 80 20 l 80 40 l 40 40 l f", "1 0 0 rg 40 20 m 80 20 l 80 40 l 40 40 l 40 20 l f", "1 0 0 rg 0 0 40 20 re f"]
        {
            let paths = record(content);
            assert_eq!(paths.len(), 1, "{content}");
            let pts = points(&paths[0]);
            let bbox = paths[0].bounding_box();
            assert!(pts.len() <= 5, "{content}: {pts:?}");
            assert!(
                pts.iter().all(|(x, y)| (*x == bbox.x0 || *x == bbox.x1) && (*y == bbox.y0 || *y == bbox.y1)),
                "{content}: every point is a corner, {pts:?}"
            );
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn guarded_interpreter_reaches_the_device_without_raster() {
        let mut device = PathDevice::default();
        execute(&mut device).unwrap();
        assert_eq!(device.paths, 1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn device_panic_is_an_execution_error_and_fresh_state_still_works() {
        assert_eq!(execute(&mut PathDevice { fail: true, ..Default::default() }), Err(craft_pdf::ExecutionError::BackendPanic));
        let mut fresh = PathDevice::default();
        execute(&mut fresh).unwrap();
        assert_eq!(fresh.paths, 1);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_pdf_is_a_typed_error() {
    assert!(matches!(craft_pdf::open_slice(b"not a pdf", None), Err(craft_pdf::OpenError::Load(_))));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn input_limit_can_be_checked_before_allocating() {
    assert!(craft_pdf::check_input_size(craft_pdf::MAX_INPUT_BYTES).is_ok());
    assert!(matches!(
        craft_pdf::check_input_size(craft_pdf::MAX_INPUT_BYTES + 1),
        Err(craft_pdf::OpenError::InputLimit { limit: craft_pdf::MAX_INPUT_BYTES })
    ));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn parsing_without_interpretation_exposes_pages_and_objects() {
    let bytes = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 40 20] >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF";
    let pdf = craft_pdf::open_slice(bytes, None).unwrap();
    assert_eq!(pdf.pages().len(), 1);
    let catalog = pdf.xref().get::<craft_pdf::syntax::object::Dict<'_>>(pdf.xref().root_id()).unwrap();
    assert!(catalog.get_ref(b"Pages").is_some());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn caller_input_policy_is_checked_before_parser_and_copy() {
    let options = craft_pdf::OpenOptions { max_input_bytes: 3, ..Default::default() };
    assert!(matches!(craft_pdf::open_slice_with_options(b"bad!", None, &options), Err(craft_pdf::OpenError::InputLimit { limit: 3 })));
    assert!(matches!(
        craft_pdf::open_with_options(std::sync::Arc::new(b"bad!".to_vec()), None, &options),
        Err(craft_pdf::OpenError::InputLimit { limit: 3 })
    ));
    assert!(craft_pdf::check_input_size_with(3, &options).is_ok());
    let larger = craft_pdf::OpenOptions { max_input_bytes: usize::MAX, ..Default::default() };
    assert!(craft_pdf::check_input_size_with(craft_pdf::MAX_INPUT_BYTES + 1, &larger).is_ok());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn caller_decode_policy_survives_open_fallback() {
    let bytes = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 40 20] >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF";
    let options =
        craft_pdf::OpenOptions { decode_limits: craft_pdf::syntax::DecodeLimits::default().with_max_decoded_stream(32), ..Default::default() };
    let low = craft_pdf::open_slice_with_options(bytes, None, &options).unwrap();
    let normal = craft_pdf::open_slice(bytes, None).unwrap();
    assert_eq!(low.decode_limits().max_decoded_stream(), 32);
    assert_eq!(normal.decode_limits(), craft_pdf::syntax::DecodeLimits::default());
}
