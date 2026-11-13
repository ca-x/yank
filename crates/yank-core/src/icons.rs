pub const ICONS: &[(&str, &str)] = &[
    (
        "clipboard",
        include_str!("../../../assets/icons/clipboard.svg"),
    ),
    ("star", include_str!("../../../assets/icons/star.svg")),
    ("monitor", include_str!("../../../assets/icons/monitor.svg")),
    (
        "list-ordered",
        include_str!("../../../assets/icons/list-ordered.svg"),
    ),
    (
        "settings",
        include_str!("../../../assets/icons/settings.svg"),
    ),
    (
        "file-text",
        include_str!("../../../assets/icons/file-text.svg"),
    ),
    ("link", include_str!("../../../assets/icons/link.svg")),
    ("image", include_str!("../../../assets/icons/image.svg")),
    ("folder", include_str!("../../../assets/icons/folder.svg")),
    ("code", include_str!("../../../assets/icons/code.svg")),
    ("palette", include_str!("../../../assets/icons/palette.svg")),
    ("trash", include_str!("../../../assets/icons/trash.svg")),
    ("search", include_str!("../../../assets/icons/search.svg")),
    (
        "checkbox",
        include_str!("../../../assets/icons/checkbox.svg"),
    ),
    (
        "arrow-right",
        include_str!("../../../assets/icons/arrow-right.svg"),
    ),
    ("users", include_str!("../../../assets/icons/users.svg")),
    (
        "rotate-cw",
        include_str!("../../../assets/icons/rotate-cw.svg"),
    ),
    (
        "download",
        include_str!("../../../assets/icons/download.svg"),
    ),
    ("copy", include_str!("../../../assets/icons/copy.svg")),
    (
        "shield-check",
        include_str!("../../../assets/icons/shield-check.svg"),
    ),
];

pub fn svg(name: &str) -> Option<&'static str> {
    ICONS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, svg)| *svg)
}

pub fn kind(kind: &str) -> &'static str {
    match kind {
        "text" => "file-text",
        "link" => "link",
        "image" => "image",
        "files" => "folder",
        "code" => "code",
        "color" => "palette",
        _ => "clipboard",
    }
}
