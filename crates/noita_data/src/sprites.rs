//! Sprite definitions (`<Sprite>` with `<RectAnimation>` children), e.g.
//! `data/enemies_gfx/player.xml`. RectAnimation fields per the serializer at 0xdafc80.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct RectAnimation {
    pub name: String,
    pub pos_x: u32,
    pub pos_y: u32,
    pub frame_count: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frames_per_row: u32,
    /// Seconds per frame.
    pub frame_wait: f32,
    /// Absent in the data for obviously looping animations (walk), so the default is 1.
    pub looping: bool,
    pub shrink_by_one_pixel: bool,
    /// Played when this animation finishes and the finished state has no follow-up.
    pub next_animation: String,
}

impl RectAnimation {
    /// Source rect `(x, y, w, h)` of frame `i` in the sprite sheet.
    pub fn frame_rect(&self, i: u32) -> (u32, u32, u32, u32) {
        let row = self.frames_per_row.max(1);
        let shrink = u32::from(self.shrink_by_one_pixel);
        (
            self.pos_x + (i % row) * self.frame_width,
            self.pos_y + (i / row) * self.frame_height,
            self.frame_width - shrink,
            self.frame_height - shrink,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpriteDef {
    pub filename: String,
    pub offset_x: f32,
    pub offset_y: f32,
    pub default_animation: String,
    pub animations: Vec<RectAnimation>,
}

impl SpriteDef {
    pub fn animation(&self, name: &str) -> Option<&RectAnimation> {
        self.animations.iter().find(|a| a.name == name)
    }
}

#[derive(Debug)]
pub enum SpriteError {
    Xml(roxmltree::Error),
    NotASprite,
    Missing(&'static str, String),
    Invalid(&'static str, String),
}

impl fmt::Display for SpriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xml(e) => write!(f, "xml: {e}"),
            Self::NotASprite => write!(f, "root element is not <Sprite>"),
            Self::Missing(attr, anim) => write!(f, "{anim}: missing {attr}"),
            Self::Invalid(attr, anim) => write!(f, "{anim}: bad {attr}"),
        }
    }
}

impl std::error::Error for SpriteError {}

pub fn parse(xml: &str) -> Result<SpriteDef, SpriteError> {
    let doc = roxmltree::Document::parse(xml).map_err(SpriteError::Xml)?;
    let root = doc.root_element();
    if !root.has_tag_name("Sprite") {
        return Err(SpriteError::NotASprite);
    }
    let f32_or = |n: roxmltree::Node, k: &'static str, d: f32| -> Result<f32, SpriteError> {
        n.attribute(k).map_or(Ok(d), |v| {
            v.trim()
                .parse()
                .map_err(|_| SpriteError::Invalid(k, v.to_owned()))
        })
    };
    let mut animations = Vec::new();
    for a in root.children().filter(|n| n.has_tag_name("RectAnimation")) {
        let name = a.attribute("name").unwrap_or_default().to_owned();
        let int = |k: &'static str| -> Result<u32, SpriteError> {
            let v = a
                .attribute(k)
                .ok_or_else(|| SpriteError::Missing(k, name.clone()))?;
            v.trim()
                .parse()
                .map_err(|_| SpriteError::Invalid(k, name.clone()))
        };
        let flag = |k: &'static str, d: bool| a.attribute(k).map_or(d, |v| v.trim() != "0");
        let frame_count = int("frame_count")?;
        animations.push(RectAnimation {
            pos_x: int("pos_x")?,
            pos_y: int("pos_y")?,
            frame_width: int("frame_width")?,
            frame_height: int("frame_height")?,
            frames_per_row: int("frames_per_row")?,
            frame_count,
            frame_wait: f32_or(a, "frame_wait", 0.0)?,
            looping: flag("loop", true),
            shrink_by_one_pixel: flag("shrink_by_one_pixel", false),
            next_animation: a.attribute("next_animation").unwrap_or_default().to_owned(),
            name,
        });
    }
    Ok(SpriteDef {
        filename: root.attribute("filename").unwrap_or_default().to_owned(),
        offset_x: f32_or(root, "offset_x", 0.0)?,
        offset_y: f32_or(root, "offset_y", 0.0)?,
        default_animation: root
            .attribute("default_animation")
            .unwrap_or_default()
            .to_owned(),
        animations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<Sprite filename="a.png" offset_x="6" offset_y="14" default_animation="stand">
        <RectAnimation frame_count="6" frame_height="19" frame_wait="0.2" frame_width="12"
            frames_per_row="4" name="stand" pos_x="0" pos_y="1"></RectAnimation>
        <RectAnimation frame_count="3" frame_height="20" frame_wait="0.1" frame_width="13"
            frames_per_row="8" loop="0" name="fly" pos_x="2" pos_y="121" shrink_by_one_pixel="1" />
    </Sprite>"#;

    #[test]
    fn parses_rect_animations() {
        let s = parse(XML).unwrap();
        assert_eq!((s.offset_x, s.offset_y), (6.0, 14.0));
        let stand = s.animation("stand").unwrap();
        assert!(stand.looping);
        assert_eq!(stand.frame_rect(0), (0, 1, 12, 19));
        assert_eq!(
            stand.frame_rect(5),
            (12, 20, 12, 19),
            "wraps after frames_per_row"
        );
        let fly = s.animation("fly").unwrap();
        assert!(!fly.looping);
        assert_eq!(fly.frame_rect(1), (15, 121, 12, 19));
    }
}
