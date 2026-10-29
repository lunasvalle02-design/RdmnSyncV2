#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod theme;
mod ui;
mod worker;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RdmnSync")
            .with_app_id("rdmnsync")
            .with_inner_size([1320.0, 840.0])
            .with_min_inner_size([980.0, 640.0])
            .with_icon(app_icon()),
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native("RdmnSync", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}

/// Icono de la ventana, dibujado por código: dos flechas opuestas sobre un círculo índigo.
fn app_icon() -> egui::IconData {
    const N: usize = 64;
    let c = (N as f32 - 1.0) / 2.0;
    let mut rgba = vec![0u8; N * N * 4];
    // Flecha horizontal centrada en `cy`, apuntando a `dir` (+1 derecha, -1 izquierda).
    let arrow = |fx: f32, fy: f32, cy: f32, dir: f32| {
        let x = fx * dir;
        let dy = (fy - cy).abs();
        let shaft = (-16.0..=8.0).contains(&x) && dy <= 2.5;
        let head = x >= 4.0 && x <= 17.0 && dy <= (17.0 - x) * 0.75;
        shaft || head
    };
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32 - c, y as f32 - c);
            let r = (fx * fx + fy * fy).sqrt();
            if r > c {
                continue;
            }
            let i = (y * N + x) * 4;
            let px = if arrow(fx, fy, -8.0, 1.0) || arrow(fx, fy, 8.0, -1.0) {
                [0xFF, 0xFF, 0xFF]
            } else {
                [0x7C, 0x7F, 0xFF]
            };
            rgba[i..i + 3].copy_from_slice(&px);
            rgba[i + 3] = ((c - r) * 2.0).clamp(0.0, 1.0).mul_add(255.0, 0.0) as u8;
        }
    }
    egui::IconData { rgba, width: N as u32, height: N as u32 }
}
