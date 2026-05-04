use std::{
    env, fs,
    path::{Path, PathBuf},
};

struct EmbeddedFont {
    const_name: &'static str,
    package: &'static str,
    version: &'static str,
    file_name: &'static str,
}

const EMBEDDED_FONTS: &[EmbeddedFont] = &[
    EmbeddedFont {
        const_name: "IBM_PLEX_SANS_TEXT",
        package: "makepad-widgets",
        version: "1.0.0",
        file_name: "IBMPlexSans-Text.ttf",
    },
    EmbeddedFont {
        const_name: "IBM_PLEX_SANS_SEMIBOLD",
        package: "makepad-widgets",
        version: "1.0.0",
        file_name: "IBMPlexSans-SemiBold.ttf",
    },
    EmbeddedFont {
        const_name: "IBM_PLEX_SANS_ITALIC",
        package: "makepad-widgets",
        version: "1.0.0",
        file_name: "IBMPlexSans-Italic.ttf",
    },
    EmbeddedFont {
        const_name: "IBM_PLEX_SANS_BOLD_ITALIC",
        package: "makepad-widgets",
        version: "1.0.0",
        file_name: "IBMPlexSans-BoldItalic.ttf",
    },
    EmbeddedFont {
        const_name: "LIBERATION_MONO_REGULAR",
        package: "makepad-widgets",
        version: "1.0.0",
        file_name: "LiberationMono-Regular.ttf",
    },
    EmbeddedFont {
        const_name: "FONT_AWESOME_SOLID",
        package: "makepad-widgets",
        version: "1.0.0",
        file_name: "fa-solid-900.ttf",
    },
    EmbeddedFont {
        const_name: "LXGW_WENKAI_REGULAR",
        package: "makepad-fonts-chinese-regular",
        version: "1.0.1",
        file_name: "LXGWWenKaiRegular.ttf",
    },
    EmbeddedFont {
        const_name: "LXGW_WENKAI_REGULAR_2",
        package: "makepad-fonts-chinese-regular-2",
        version: "1.0.1",
        file_name: "LXGWWenKaiRegular.ttf.2",
    },
    EmbeddedFont {
        const_name: "LXGW_WENKAI_BOLD",
        package: "makepad-fonts-chinese-bold",
        version: "1.0.1",
        file_name: "LXGWWenKaiBold.ttf",
    },
    EmbeddedFont {
        const_name: "LXGW_WENKAI_BOLD_2",
        package: "makepad-fonts-chinese-bold-2",
        version: "1.0.1",
        file_name: "LXGWWenKaiBold.ttf.2",
    },
    EmbeddedFont {
        const_name: "NOTO_COLOR_EMOJI",
        package: "makepad-fonts-emoji",
        version: "1.0.0",
        file_name: "NotoColorEmoji.ttf",
    },
];

fn main() {
    println!("cargo:rerun-if-env-changed=YANK_MAKEPAD_REGISTRY_SRC");
    println!("cargo:rerun-if-env-changed=CARGO_HOME");
    println!("cargo:rerun-if-env-changed=HOME");
    println!("cargo:rerun-if-env-changed=USERPROFILE");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let generated_path = out_dir.join("embedded_makepad_fonts.rs");
    let mut generated = String::new();

    for font in EMBEDDED_FONTS {
        let file_path = find_resource(font);
        println!("cargo:rerun-if-changed={}", file_path.display());
        generated.push_str("const ");
        generated.push_str(font.const_name);
        generated.push_str(": &[u8] = include_bytes!(r#\"");
        generated.push_str(&file_path.to_string_lossy());
        generated.push_str("\"#);\n");
    }

    fs::write(generated_path, generated).expect("embedded font source can be written");
}

fn find_resource(font: &EmbeddedFont) -> PathBuf {
    if let Some(root) = env::var_os("YANK_MAKEPAD_REGISTRY_SRC")
        && let Some(path) = find_resource_in_registry_src(Path::new(&root), font)
    {
        return path;
    }

    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .or_else(|| env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".cargo")))
        .expect("CARGO_HOME or HOME is required to locate Makepad resources");

    let registry_src = cargo_home.join("registry").join("src");
    find_resource_in_registry_src(&registry_src, font).unwrap_or_else(|| {
        panic!(
            "could not find {}/resources/{} in {}",
            font.package,
            font.file_name,
            registry_src.display()
        )
    })
}

fn find_resource_in_registry_src(registry_src: &Path, font: &EmbeddedFont) -> Option<PathBuf> {
    let package_dir_name = format!("{}-{}", font.package, font.version);
    for registry in fs::read_dir(registry_src).ok()? {
        let Ok(registry) = registry else {
            continue;
        };
        let resource = registry
            .path()
            .join(&package_dir_name)
            .join("resources")
            .join(font.file_name);
        if resource.is_file() {
            return Some(resource);
        }
    }
    None
}
