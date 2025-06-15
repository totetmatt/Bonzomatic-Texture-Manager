#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)]

use eframe::egui::{Color32, Stroke, Style, Theme, style::Selection};
use eframe::{
    egui,
    epaint::text::{FontInsert, InsertFontFamily},
};
use image::EncodableLayout;
use serde_json::Value;
use std::fs::copy;
use std::io::Write;
use zip::write::SimpleFileOptions;

use std::fs::{File, read_dir};
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use zip::read::ZipArchive;
use zip::write::ZipWriter;
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}
fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../icon.png"))
        .expect("The icon data must be valid");

    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_icon(Arc::new(icon))
            .with_resizable(false),
        ..Default::default()
    };
    // Cmd line argument
    // bonzomatic-texture-manager [BONZOMATIC_ROOT_PATH]

    // BONZOMATIC_ROOT_PATH, by default check "./", but can override
    let bonzomatic_root_path = PathBuf::from(std::env::args().nth(1).unwrap_or(".".to_owned()));
    eframe::run_native(
        "Bonzomatic Texture Manager",
        options,
        Box::new(|cc| {
            Ok(Box::new(BonzomaticTextureManagerApp::new(
                cc,
                bonzomatic_root_path,
            )))
        }),
    )
}
// Demonstrates how to add a font to the existing ones
fn add_font(ctx: &egui::Context) {
    ctx.add_font(FontInsert::new(
        "DepartureMono",
        egui::FontData::from_static(include_bytes!("../DepartureMono-Regular.otf")),
        vec![
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: egui::epaint::text::FontPriority::Highest,
            },
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: egui::epaint::text::FontPriority::Lowest,
            },
        ],
    ));
}

fn use_style(style: &mut Style) {
    let orange = Color32::from_rgb(250, 128, 32);
    style.visuals.hyperlink_color = orange;
    style.visuals.text_cursor.stroke.color = orange;
    style.visuals.override_text_color = Some(orange);
    style.visuals.selection = Selection {
        bg_fill: orange,
        stroke: Stroke::new(1.0, orange),
    };
}
fn setup_custom_style(ctx: &egui::Context) {
    ctx.style_mut_of(Theme::Light, use_style);
    ctx.style_mut_of(Theme::Dark, use_style);
}
impl BonzomaticTextureManagerApp {
    fn new(cc: &eframe::CreationContext<'_>, bonzomatic_root_path: PathBuf) -> Self {
        setup_custom_style(&cc.egui_ctx);
        add_font(&cc.egui_ctx);
        let bonzomatic_dir = std::fs::canonicalize(bonzomatic_root_path).unwrap();
        let mut instance = Self {
            bonzomatic_dir: bonzomatic_dir.into_os_string().into_string().unwrap(),
            local_textures: Vec::new(),
            changed: false,
        };
        instance.update_texture_list();
        instance
    }

    fn from_bonzomatic_dir(&self, path: &String) -> PathBuf {
        let mut dir_path = PathBuf::new();
        dir_path.push(&self.bonzomatic_dir);
        dir_path.push(path);
        dir_path
    }
    fn get_config_file_path(&self) -> PathBuf {
        self.from_bonzomatic_dir(&"config.json".to_string())
    }
    fn get_texture_dir_path(&self) -> PathBuf {
        self.from_bonzomatic_dir(&"textures/".to_string())
    }
    fn get_config_as_json(&self) -> Value {
        let config_json = self.get_config_file_path();

        let file = File::open(config_json).unwrap();
        let mut reader = BufReader::new(file);
        let mut content = String::new();
        let _ = reader.read_to_string(&mut content);

        let content: String = content
            .lines()
            .map(|line| {
                let c = line.rfind("\"");
                match line.rfind("//") {
                    Some(r) if c.map(|x| x < r).unwrap_or(true) => {
                        format!("{}\n", &line[0..r])
                    }
                    _ => format!("{}\n", line),
                }
            })
            .collect();

        serde_json::from_str(&content).unwrap()
    }
    fn update_texture_list(&mut self) {
        self.local_textures.clear();
        if !self.get_texture_dir_path().exists() {
            return;
        }
        let config = self.get_config_as_json();

        let textures = &config["textures"];
        for (a, b) in textures.as_object().unwrap().iter() {
            self.local_textures.push(BonzomaticTexture::new(
                a.to_string(),
                b.as_str().unwrap().to_string(),
                true,
            ));
        }

        let textures_unused = read_dir(self.get_texture_dir_path()).unwrap();
        for texture in textures_unused {
            let t = texture.unwrap().path();
            let texture_path = t.strip_prefix(&self.bonzomatic_dir).unwrap();
            let texture_file = texture_path.file_name().unwrap().to_str().unwrap();
            let texture_name = texture_path.file_stem().unwrap().to_str().unwrap();
            let texture_name = format!("tex{}", capitalize(texture_name));
            if !self
                .local_textures
                .iter()
                .any(|x| x.glsl_name.eq(&texture_name) || x.local_file.ends_with(&texture_file))
            {
                self.local_textures.push(BonzomaticTexture::new(
                    texture_name,
                    texture_path.as_os_str().to_os_string().into_string().unwrap(),
                    false,
                ));
            }
        }
    }
    fn  config_backup(&self){
        let backup =self.get_config_file_path().with_file_name("_bk_config.json");
        if backup.exists() {return;  }

        let _ = std::fs::copy(self.get_config_file_path(), backup);
    }
    fn write_config_json(&self) {
        self.config_backup();
        let mut config = self.get_config_as_json();
        let textures = self
            .local_textures
            .iter()
            .filter(|x| x.activated)
            .map(|x| (x.glsl_name.clone(), Value::String(x.local_file.clone())))
            .collect::<serde_json::Map<String, Value>>();
        config["textures"] = Value::Object(textures);

        let file = File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open(self.get_config_file_path())
            .unwrap();
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &config).unwrap();
    }
    fn write_zip_pack(&self) {
        let mut zip_path = PathBuf::new();
        zip_path.push(&self.bonzomatic_dir);
        zip_path.push("texture_pack.zip");
        let file = File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open(zip_path)
            .unwrap();

        let mut zip = ZipWriter::new(file);

        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for textures in self.local_textures.iter().filter(|x| x.activated) {
            let texture_path = self.from_bonzomatic_dir(&textures.local_file);
            let file = File::open(&texture_path).unwrap();
            zip.start_file(&textures.local_file, options).unwrap();
            let mut buffer = Vec::new();
            std::io::copy(&mut file.take(u64::MAX), &mut buffer).unwrap();
            zip.write_all(&buffer).unwrap();
        }
        zip.finish().unwrap();
    }
}
struct BonzomaticTextureManagerApp {
    bonzomatic_dir: String,
    local_textures: Vec<BonzomaticTexture>,
    changed: bool,
}

struct BonzomaticTexture {
    glsl_name: String,
    local_file: String,
    activated: bool,
}
impl BonzomaticTexture {
    fn new(glsl_name: String, local_file: String, activated: bool) -> Self {
        BonzomaticTexture {
            glsl_name,
            local_file,
            activated,
        }
    }
}

impl eframe::App for BonzomaticTextureManagerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.changed {
            self.changed = false;
            self.write_config_json();
            self.update_texture_list();
        }
        ctx.input(|input| {
            for file in input.raw.dropped_files.iter() {
                // Event that detect file dropped to the Window

                // Making sure it wond't collide with existing file
                let source_file_path = file.path.as_ref().unwrap();
                let extension = source_file_path.extension().unwrap();
                let extension = extension.to_str().unwrap().to_ascii_lowercase();
                if extension.ends_with("png")
                    && extension.ends_with("jpg")
                    && extension.ends_with("jpeg")
                {
                    let mut dest_file_name = source_file_path
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned();
                    let mut i = 2;
                    loop {
                        if !self.local_textures.iter().any(|x| {
                            x.local_file
                                .to_lowercase()
                                .ends_with(&dest_file_name.to_lowercase())
                        }) {
                            break;
                        }
                        let file_stem = source_file_path.file_stem().unwrap().to_str().unwrap();
                        let file_ext = source_file_path.extension().unwrap().to_str().unwrap();
                        dest_file_name = String::new();
                        dest_file_name.push_str(&file_stem);
                        dest_file_name.push_str(format!("_{i}.").as_str());
                        dest_file_name.push_str(&file_ext);
                        i = i + 1;
                    }

                    let mut dest_file_path = PathBuf::new();
                    dest_file_path.push(&self.bonzomatic_dir);
                    dest_file_path.push("textures");
                    dest_file_path.push(&dest_file_name);

                    let _ = copy(source_file_path, dest_file_path);
                    self.update_texture_list();
                }
                if extension.ends_with("zip") {
                    println!("{source_file_path:?}");
                    let file = File::open(source_file_path).unwrap();
                    let mut archive = ZipArchive::new(file).unwrap();

                    // Iterate through all the files in the ZIP archive.

                    for i in 0..archive.len() {
                        let mut file = archive.by_index(i).unwrap();
                        if file.is_dir() {
                            continue;
                        }
                        if file.is_file() && !file.name().starts_with("textures/") {
                            continue;
                        }
                        println!("File name: {}", file.name());
                        let mut buffer = Vec::new();
                        file.read_to_end(&mut buffer).unwrap();
                        let dest_file = file.name().to_owned();
                        let dest_file = self.from_bonzomatic_dir(&dest_file);

                        if dest_file.exists() {
                            continue;
                        }
                        let file = File::options()
                            .create(true)
                            .write(true)
                            .truncate(true)
                            .open(dest_file)
                            .unwrap();
                        let mut writer = BufWriter::new(file);
                        let _ = writer.write_all(buffer.as_bytes());
                    }
                    self.update_texture_list();
                }
            }
        });
        egui_extras::install_image_loaders(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Bonzomatic Texture Manager");
            ui.horizontal(|ui| {
                ui.label(&self.bonzomatic_dir);
            });
            egui::ScrollArea::vertical()
                .drag_to_scroll(true)
                .max_height(475.)
                .show(ui, |ui| {
                    for bonzomatic_texture in self.local_textures.iter_mut() {
                        ui.horizontal(|ui| {
                            ui.set_height(48.0);
                            if ui.checkbox(&mut bonzomatic_texture.activated, "").changed() {
                                self.changed = true;
                            };
                            ui.text_edit_singleline(&mut bonzomatic_texture.glsl_name);
                            ui.text_edit_singleline(&mut bonzomatic_texture.local_file);

                            let mut file_path = PathBuf::new();
                            file_path.push(&self.bonzomatic_dir);
                            file_path.push(&bonzomatic_texture.local_file);
                            let image_path = format!("file://{}", file_path.to_str().unwrap());

                            ui.image(image_path);
                        });
                    }
                    // Add a lot of widgets here.
                });
            if !self.local_textures.is_empty() {
                if ui.button("Export as zip package").clicked() {
                    self.write_zip_pack();
                }
            }
        });
    }
}
