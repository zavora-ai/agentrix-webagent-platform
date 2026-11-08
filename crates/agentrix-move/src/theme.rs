//! The converted site's theme: the experience lane's `templates/site/page.html` and
//! `static/site/site.css` (S1-09), filled with the site's tokens and each rebuilt page.
//!
//! The template and stylesheet are the experience lane's files, read at compile time, so the
//! Move and the edge agree on one theme.

use minijinja::Environment;
use serde::Serialize;

use crate::clean::NavLink;

const PAGE_TEMPLATE: &str = include_str!("../../agentrix-console/templates/site/page.html");

/// The theme's stylesheet, copied into every release as `assets/site.css`.
pub const SITE_CSS: &[u8] = include_bytes!("../../agentrix-console/static/site/site.css");

/// The site's look: colours and fonts (06 §4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Tokens {
    /// Page background.
    pub bg: String,
    /// A softer background for panels.
    pub soft: String,
    /// Body text.
    pub ink: String,
    /// Secondary text.
    pub ink2: String,
    /// Rules and borders.
    pub line: String,
    /// The brand colour, for links and buttons.
    pub accent: String,
    /// Text on the brand colour.
    pub on_accent: String,
    /// The display font stack.
    pub display: String,
    /// The text font stack.
    pub text: String,
}

impl Default for Tokens {
    fn default() -> Self {
        Self {
            bg: "#fffdf8".into(),
            soft: "#f6efe4".into(),
            ink: "#1f1a14".into(),
            ink2: "#5b4f42".into(),
            line: "#e6dccd".into(),
            accent: "#8a3b12".into(),
            on_accent: "#ffffff".into(),
            display: "Georgia, 'Times New Roman', serif".into(),
            text: "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif".into(),
        }
    }
}

fn hex_rgb(hex: &str) -> Option<(f64, f64, f64)> {
    let h = hex.trim_start_matches('#');
    let full = match h.len() {
        3 => h.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => h.to_owned(),
        _ => return None,
    };
    let v = u32::from_str_radix(&full, 16).ok()?;
    Some((
        f64::from((v >> 16) & 0xff) / 255.0,
        f64::from((v >> 8) & 0xff) / 255.0,
        f64::from(v & 0xff) / 255.0,
    ))
}

fn luminance(hex: &str) -> Option<f64> {
    let (r, g, b) = hex_rgb(hex)?;
    let lin = |c: f64| {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    Some(0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b))
}

/// The WCAG contrast ratio of two hex colours.
pub fn contrast(a: &str, b: &str) -> f64 {
    match (luminance(a), luminance(b)) {
        (Some(x), Some(y)) => (x.max(y) + 0.05) / (x.min(y) + 0.05),
        _ => 1.0,
    }
}

impl Tokens {
    /// Tokens that keep the old site's brand colour, read from its stylesheet as text: the
    /// most used saturated colour becomes the accent, when it keeps AA contrast (4.5:1) on
    /// the background. Everything else stays at the accessible defaults.
    pub fn from_css(css: &str) -> Self {
        let mut tokens = Self::default();
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        let bytes = css.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'#' {
                let hex: String = css[i + 1..]
                    .chars()
                    .take_while(char::is_ascii_hexdigit)
                    .collect();
                if hex.len() == 6 || hex.len() == 3 {
                    *counts
                        .entry(format!("#{}", hex.to_ascii_lowercase()))
                        .or_default() += 1;
                }
                i += 1 + hex.len();
            } else {
                i += 1;
            }
        }
        let saturated = |hex: &str| {
            hex_rgb(hex).is_some_and(|(r, g, b)| {
                let max = r.max(g).max(b);
                let min = r.min(g).min(b);
                max - min > 0.25
            })
        };
        if let Some((accent, _)) = counts
            .iter()
            .filter(|(hex, _)| saturated(hex) && contrast(hex, &tokens.bg) >= 4.5)
            .max_by_key(|(_, n)| **n)
        {
            tokens.accent = accent.clone();
            tokens.on_accent = if contrast(accent, "#ffffff") >= 4.5 {
                "#ffffff".into()
            } else {
                tokens.ink.clone()
            };
        }
        tokens
    }
}

/// The site, as the template names it.
#[derive(Debug, Clone, Serialize)]
pub struct SiteView {
    /// The business name.
    pub name: String,
    /// The domain.
    pub domain: String,
    /// The language.
    pub lang: String,
    /// The look.
    pub tokens: Tokens,
    /// The menu.
    pub nav: Vec<NavLink>,
    /// The contact page, when there is one.
    pub contact_href: Option<String>,
    /// Zavora's fixture site (R3.3).
    pub fixture: bool,
    /// Whether the site counts visits (it does not, this weekend).
    pub counting: bool,
}

/// One page, as the template names it.
#[derive(Debug, Clone, Serialize)]
pub struct PageView<'a> {
    /// Its title.
    pub title: &'a str,
    /// Its description.
    pub description: Option<&'a str>,
    /// Its path.
    pub path: &'a str,
    /// Its Markdown twin.
    pub md_href: String,
    /// The rebuilt body, checked by [`crate::rebuild::check_body`].
    pub body_html: &'a str,
    /// When it was last changed, in words.
    pub updated: Option<String>,
}

/// Where a page's files go in the release: `/` is `index.html`, `/about/` is
/// `about/index.html`.
pub fn page_file(path: &str) -> String {
    let trimmed = path
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_matches('/');
    if trimmed.is_empty() {
        "index.html".into()
    } else {
        format!("{trimmed}/index.html")
    }
}

/// The page's Markdown twin, beside its HTML.
pub fn twin_file(path: &str) -> String {
    page_file(path).trim_end_matches(".html").to_owned() + ".md"
}

/// Renders one page with the site theme.
pub fn render_page(site: &SiteView, page: &PageView<'_>) -> anyhow::Result<String> {
    let mut env = Environment::new();
    env.add_template("page.html", PAGE_TEMPLATE)?;
    let html = env
        .get_template("page.html")?
        .render(minijinja::context! { site => site, page => page })?;
    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_files_and_twins() {
        assert_eq!(page_file("/"), "index.html");
        assert_eq!(page_file("/about/"), "about/index.html");
        assert_eq!(
            page_file("/2026/10/fresh-bread"),
            "2026/10/fresh-bread/index.html"
        );
        assert_eq!(twin_file("/about/"), "about/index.md");
    }

    #[test]
    fn the_brand_colour_is_kept_only_with_enough_contrast() {
        let css = "a { color: #1d4ed8 } .btn { background: #1d4ed8 } .hint { color: #ffff00 }";
        assert_eq!(Tokens::from_css(css).accent, "#1d4ed8");
        assert_eq!(
            Tokens::from_css(".x { color: #ffff00 }").accent,
            Tokens::default().accent
        );
    }

    #[test]
    fn renders_the_experience_lanes_template_with_escaping() {
        let site = SiteView {
            name: "Kilele <Bakery>".into(),
            domain: "kilele-bakery.example".into(),
            lang: "en".into(),
            tokens: Tokens::default(),
            nav: vec![NavLink {
                label: "Home".into(),
                href: "/".into(),
            }],
            contact_href: None,
            fixture: true,
            counting: false,
        };
        let page = PageView {
            title: "Home",
            description: None,
            path: "/",
            md_href: "/index.md".into(),
            body_html: "<h1>Kilele</h1>",
            updated: None,
        };
        let html = render_page(&site, &page).unwrap();
        assert!(html.contains("<h1>Kilele</h1>"));
        assert!(html.contains("Kilele &lt;Bakery&gt;"));
        assert!(html.contains("Zavora&#x27;s fixture") || html.contains("Zavora's fixture"));
    }
}
