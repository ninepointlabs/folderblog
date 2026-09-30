//! Regenerate themes/default/assets/highlight.css:
//! cargo run --example highlight_css > themes/default/assets/highlight.css
use syntect::highlighting::ThemeSet;
use syntect::html::{ClassStyle, css_for_theme_with_class_style};

fn main() {
    let ts = ThemeSet::load_defaults();
    let style = ClassStyle::SpacedPrefixed { prefix: "hl-" };
    let light = css_for_theme_with_class_style(&ts.themes["InspiredGitHub"], style).unwrap();
    let dark = css_for_theme_with_class_style(&ts.themes["base16-ocean.dark"], style).unwrap();
    println!("/* Syntax highlighting: classes emitted by folderblog (hl-*). Generated from syntect themes. */");
    println!("{light}");
    println!("@media (prefers-color-scheme: dark) {{\n{dark}\n}}");
}
