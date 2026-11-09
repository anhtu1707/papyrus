use eframe::egui::{Color32, Painter, Pos2, Rect, Stroke};

pub const COLOR_DESK: Color32 = Color32::from_rgb(38, 33, 28);
pub const COLOR_PARCHMENT: Color32 = Color32::from_rgb(250, 246, 237);
pub const COLOR_KRAFT: Color32 = Color32::from_rgb(223, 211, 191);
pub const COLOR_KRAFT_DARK: Color32 = Color32::from_rgb(196, 180, 155);
pub const COLOR_INK: Color32 = Color32::from_rgb(30, 26, 22);
pub const COLOR_INK_MUTED: Color32 = Color32::from_rgb(110, 95, 78);
pub const COLOR_WAX_RED: Color32 = Color32::from_rgb(200, 62, 45);
pub const COLOR_STAMP_BLUE: Color32 = Color32::from_rgb(27, 94, 112);
pub const COLOR_STAMP_GREEN: Color32 = Color32::from_rgb(0, 135, 90);

/// Draws a realistic jagged torn paper edge along the bottom edge of a rect
pub fn draw_torn_bottom(painter: &Painter, rect: Rect, fill: Color32, border_color: Color32) {
    let mut points = Vec::new();
    points.push(rect.left_top());
    points.push(rect.right_top());

    let right = rect.right();
    let left = rect.left();
    let bottom = rect.bottom();
    let steps = 40;
    let step_x = (right - left) / steps as f32;

    for i in 0..=steps {
        let x = right - (i as f32 * step_x);
        let offset_y = if i % 2 == 0 {
            -6.0 + ((i * 7) % 5) as f32
        } else {
            0.0 - ((i * 3) % 4) as f32
        };
        points.push(Pos2::new(x, bottom + offset_y));
    }

    painter.add(eframe::egui::Shape::convex_polygon(
        points.clone(),
        fill,
        Stroke::NONE,
    ));

    // Draw jagged stroke along the tear
    for i in 2..points.len() - 1 {
        painter.line_segment([points[i], points[i + 1]], Stroke::new(1.0, border_color));
    }
}

/// Draws an airmail / postage stamp with serrated style
pub fn draw_postage_stamp(
    painter: &Painter,
    rect: Rect,
    border_color: Color32,
    bg_color: Color32,
) {
    painter.rect_filled(rect, 2.0, bg_color);
    painter.rect_stroke(rect, 2.0, Stroke::new(1.5, border_color), eframe::egui::StrokeKind::Inside);

    // Draw small perforation notch marks around perimeter
    let notch_color = COLOR_DESK;
    let mut x = rect.left() + 6.0;
    while x < rect.right() - 4.0 {
        painter.circle_filled(Pos2::new(x, rect.top()), 1.5, notch_color);
        painter.circle_filled(Pos2::new(x, rect.bottom()), 1.5, notch_color);
        x += 8.0;
    }
    let mut y = rect.top() + 6.0;
    while y < rect.bottom() - 4.0 {
        painter.circle_filled(Pos2::new(rect.left(), y), 1.5, notch_color);
        painter.circle_filled(Pos2::new(rect.right(), y), 1.5, notch_color);
        y += 8.0;
    }
}

/// Draws a sealing wax stamp button
pub fn draw_wax_seal(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    color: Color32,
) {
    // Drop shadow
    painter.circle_filled(center + Pos2::new(0.0, 2.0).to_vec2(), radius + 1.0, Color32::from_black_alpha(70));
    // Main seal base
    painter.circle_filled(center, radius, color);
    // Subtle inner bevel highlight
    painter.circle_stroke(center, radius - 2.0, Stroke::new(1.2, Color32::from_white_alpha(90)));
}

/// Draws a washi tape sticker strip holding down paper
pub fn draw_washi_tape(
    painter: &Painter,
    rect: Rect,
) {
    let tape_color = Color32::from_rgba_premultiplied(235, 222, 175, 180);
    painter.rect_filled(rect, 1.0, tape_color);
    painter.line_segment(
        [rect.left_top(), rect.left_bottom()],
        Stroke::new(1.0, Color32::from_black_alpha(35)),
    );
    painter.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, Color32::from_black_alpha(35)),
    );
}

/// Draws a metallic paperclip holding a tab
pub fn draw_paperclip(painter: &Painter, top_left: Pos2) {
    let color = Color32::from_rgb(122, 109, 94);
    let stroke = Stroke::new(1.8, color);

    let p1 = top_left + Pos2::new(4.0, 8.0).to_vec2();
    let p2 = top_left + Pos2::new(4.0, 20.0).to_vec2();
    let p3 = top_left + Pos2::new(12.0, 20.0).to_vec2();
    let p4 = top_left + Pos2::new(12.0, 4.0).to_vec2();
    let p5 = top_left + Pos2::new(8.0, 4.0).to_vec2();
    let p6 = top_left + Pos2::new(8.0, 16.0).to_vec2();

    painter.line_segment([p1, p2], stroke);
    painter.line_segment([p2, p3], stroke);
    painter.line_segment([p3, p4], stroke);
    painter.line_segment([p4, p5], stroke);
    painter.line_segment([p5, p6], stroke);
}
