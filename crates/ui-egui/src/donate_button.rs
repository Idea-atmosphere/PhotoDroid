//! Phone settings Donate button: a gradient capsule and a gently beating heart.
//! No network access or payment data is involved in drawing the button.

use egui::{Color32, CornerRadius, Mesh, Pos2, Response, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

/// Both hearts use the same embedded, anti-aliased SVG rather than a concave triangle fan.
pub(crate) fn paint_heart(ui: &Ui, center: Pos2, size: f32) {
    let rect = egui::Rect::from_center_size(center, vec2(size, size));
    egui::Image::new(egui::include_image!("../../../assets/icons/donate-gradient-heart.svg"))
        .fit_to_exact_size(vec2(size, size))
        .paint_at(ui, rect);
}

fn paint_gradient_ring(painter: &egui::Painter, rect: egui::Rect) {
    paint_three_color_ring(painter, rect, false);
}

/// Ring for the popup's Copy button; the final stop is pink rather than purple.
pub(crate) fn paint_three_color_ring(painter: &egui::Painter, rect: egui::Rect, pink_end: bool) {
    use std::f32::consts::{FRAC_PI_2, PI};
    const STEPS: usize = 24;
    const WIDTH: f32 = 2.0;
    let radius = rect.height() / 2.0;
    let left_center = pos2(rect.left() + radius, rect.center().y);
    let right_center = pos2(rect.right() - radius, rect.center().y);
    let left = Color32::from_rgb(0x38, 0xbd, 0xf8);
    let right = Color32::from_rgb(0x63, 0x66, 0xf1);
    let mut ring = Vec::with_capacity((STEPS + 1) * 2);
    for i in 0..=STEPS {
        let a = -FRAC_PI_2 + PI * i as f32 / STEPS as f32;
        ring.push((right_center, vec2(a.cos(), a.sin())));
    }
    for i in 0..=STEPS {
        let a = FRAC_PI_2 + PI * i as f32 / STEPS as f32;
        ring.push((left_center, vec2(a.cos(), a.sin())));
    }
    let mut mesh = Mesh::default();
    for (center, direction) in &ring {
        let outer = *center + *direction * radius;
        let t = ((outer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let color = if pink_end {
            let pink = Color32::from_rgb(0xec, 0x48, 0x99);
            if t < 0.55 { left.lerp_to_gamma(right, t / 0.55) } else { right.lerp_to_gamma(pink, (t - 0.55) / 0.45) }
        } else {
            left.lerp_to_gamma(right, t)
        };
        mesh.colored_vertex(outer, color);
        mesh.colored_vertex(*center + *direction * (radius - WIDTH), color);
    }
    for i in 0..ring.len() as u32 {
        let a = i * 2;
        let b = ((i + 1) % ring.len() as u32) * 2;
        mesh.add_triangle(a, a + 1, b);
        mesh.add_triangle(a + 1, b + 1, b);
    }
    painter.add(Shape::mesh(mesh));
}

pub(crate) fn heartbeat(time: f64) -> f32 {
    let phase = time.rem_euclid(1.4) as f32 / 1.4;
    let bump = |center: f32, width: f32| (-((phase - center) / width).powi(2)).exp();
    1.0 + 0.10 * bump(0.10, 0.045) + 0.07 * bump(0.30, 0.05)
}

/// A single accessible 48-point touch target, drawn only while settings are open.
pub(crate) fn button(ui: &mut Ui) -> Response {
    let text_color = Color32::from_rgb(0xe8, 0xee, 0xf7);
    let galley = ui.painter().layout_no_wrap(tl!("Donate").to_owned(), crate::theme::semibold(16.0), text_color);
    let (padding, gap, heart_size) = (22.0, 8.0, 20.0);
    let width = (padding * 2.0 + galley.size().x + gap + heart_size).max(136.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 48.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), tl!("Donate")));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let fill =
            if response.hovered() || response.is_pointer_button_down_on() { Color32::from_rgb(0x14, 0x1d, 0x33) } else { Color32::from_rgb(0x0d, 0x14, 0x24) };
        painter.rect_filled(rect, CornerRadius::same(24), fill);
        paint_gradient_ring(painter, rect);
        if response.has_focus() {
            painter.rect_stroke(rect.expand(3.0), CornerRadius::same(27), Stroke::new(2.0, Color32::WHITE), StrokeKind::Outside);
        }
        let content_width = galley.size().x + gap + heart_size;
        let content_left = rect.center().x - content_width / 2.0;
        let text_y = rect.center().y - galley.size().y / 2.0;
        let heart_center = pos2(content_left + galley.size().x + gap + heart_size / 2.0, rect.center().y);
        painter.galley(pos2(content_left, text_y), galley, text_color);
        // Respect an egui style that disables animation; the Android system setting is not read here.
        let animate = ui.style().animation_time > 0.0;
        let scale = if animate { heartbeat(ui.input(|i| i.time)) } else { 1.0 };
        paint_heart(ui, heart_center, heart_size * scale);
        if animate {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_is_periodic_and_subtle() {
        assert!((heartbeat(0.14) - 1.10).abs() < 0.002);
        assert!((heartbeat(0.42) - 1.07).abs() < 0.002);
        assert!((heartbeat(0.14) - heartbeat(1.54)).abs() < 0.0001);
        for i in 0..140 {
            let scale = heartbeat(i as f64 / 100.0);
            assert!((1.0..=1.101).contains(&scale));
        }
    }

    #[test]
    fn embedded_heart_svg_rasterizes_with_gradient_and_a_complete_center() {
        let bytes = include_bytes!("../../../assets/icons/donate-gradient-heart.svg");
        let image = egui_extras::image::load_svg_bytes_with_size(
            bytes,
            egui::load::SizeHint::Size { width: 64, height: 64, maintain_aspect_ratio: true },
            &Default::default(),
        ).expect("donation heart SVG should render");
        let pixel = |x: usize, y: usize| image.pixels[y * image.size[0] + x];
        assert_eq!(pixel(0, 0).a(), 0, "outside the heart is transparent");
        assert!(pixel(32, 32).a() > 240, "the center of the heart is filled");
        assert!(pixel(32, 45).a() > 240, "the lower heart is filled");
        assert_ne!(pixel(32, 32), pixel(32, 45), "heart uses a vertical gradient");
    }

    #[test]
    fn gradient_ring_has_both_endpoint_colors() {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(320.0, 240.0))), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            let _ = button(ui);
        });
        let meshes: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                Shape::Mesh(mesh) => Some(mesh),
                _ => None,
            })
            .collect();
        assert!(!meshes.is_empty(), "gradient ring mesh was drawn");
        let ring = meshes[0];
        assert!(ring.vertices.iter().any(|v| v.color == Color32::from_rgb(0x38, 0xbd, 0xf8)));
        assert!(ring.vertices.iter().any(|v| v.color == Color32::from_rgb(0x63, 0x66, 0xf1)));
        output.textures_delta.clear();
    }
}
