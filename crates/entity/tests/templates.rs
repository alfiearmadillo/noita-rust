//! Entity template rules on in-memory files.

use std::collections::HashMap;

use entity::{Files, LoadError, Loader};

struct Mem(HashMap<&'static str, &'static str>);

impl Files for Mem {
    fn read(&self, path: &str) -> Option<String> {
        self.0.get(path).map(|s| (*s).to_owned())
    }
}

fn loader(files: &[(&'static str, &'static str)]) -> Loader<Mem> {
    Loader::new(Mem(files.iter().copied().collect()))
}

#[test]
fn components_attributes_and_transform_are_read() {
    let mut l = loader(&[(
        "a.xml",
        r#"<?xml version="1.0"?>
<!-- header -- with dashes -->
<Entity name="a" tags="x, y" >
  <_Transform position.x="3" position.y="4" />
  <VelocityComponent gravity_y="400" >
    <mLatestPos x="1" ></mLatestPos>
  </VelocityComponent>
  <Entity name="child"/>
</Entity>"#,
    )]);
    let e = l.load("a.xml").unwrap();
    assert_eq!(e.name(), "a");
    assert_eq!(e.tags(), ["x", "y"]);
    assert_eq!(e.transform.as_ref().unwrap().attr("position.x"), Some("3"));
    let v = e.component("VelocityComponent").unwrap();
    assert_eq!(v.attr("gravity_y"), Some("400"));
    assert_eq!(v.child("mLatestPos").unwrap().attr("x"), Some("1"));
    assert_eq!(e.children[0].name(), "child");
}

#[test]
fn the_reader_tolerates_comments_in_tags_and_repeated_attributes() {
    let mut l = loader(&[(
        "a.xml",
        r#"<Entity>
  <AIComponent
     a="1"
     <!-- b="2" -->
     c="3"
     c="4"
  ></AIComponent>
</Entity>"#,
    )]);
    let e = l.load("a.xml").unwrap();
    let c = e.component("AIComponent").unwrap();
    assert_eq!(c.attr("a"), Some("1"));
    assert_eq!(c.attr("b"), None);
    assert_eq!(c.attr("c"), Some("4"), "the last value wins");
}

#[test]
fn base_overrides_match_components_in_order() {
    let mut l = loader(&[
        (
            "base.xml",
            r#"<Entity name="base" tags="one">
  <Sprite image="a" z="1" />
  <Sprite image="b" z="2" />
  <Life hp="10" ><mult fire="1" ice="1" /></Life>
</Entity>"#,
        ),
        (
            "derived.xml",
            r#"<Entity name="derived" tags="two">
  <Base file="base.xml" >
    <Sprite image="a2" />
    <Sprite z="9" />
    <Life hp="20" ><mult ice="3" /></Life>
    <Extra value="1" />
  </Base>
  <Sprite image="own" />
</Entity>"#,
        ),
    ]);
    let e = l.load("derived.xml").unwrap();
    assert_eq!(e.name(), "derived");
    assert_eq!(e.tags(), ["one", "two"]);
    let sprites: Vec<_> = e.components_of("Sprite").collect();
    assert_eq!(sprites.len(), 3);
    assert_eq!(
        (sprites[0].attr("image"), sprites[0].attr("z")),
        (Some("a2"), Some("1"))
    );
    assert_eq!(
        (sprites[1].attr("image"), sprites[1].attr("z")),
        (Some("b"), Some("9"))
    );
    assert_eq!(sprites[2].attr("image"), Some("own"));
    let life = e.component("Life").unwrap();
    assert_eq!(life.attr("hp"), Some("20"));
    let mult = life.child("mult").unwrap();
    assert_eq!(
        (mult.attr("fire"), mult.attr("ice")),
        (Some("1"), Some("3"))
    );
    // A component the base lacks is appended.
    assert!(e.component("Extra").is_some());
}

#[test]
fn children_come_from_bases_only_when_asked() {
    let files = [
        (
            "base.xml",
            r#"<Entity><Entity name="kid"/><Thing a="1"/></Entity>"#,
        ),
        ("without.xml", r#"<Entity><Base file="base.xml"/></Entity>"#),
        (
            "with.xml",
            r#"<Entity><Base file="base.xml" include_children="1"><Entity name="extra"/></Base></Entity>"#,
        ),
    ];
    let mut l = loader(&files);
    assert!(l.load("without.xml").unwrap().children.is_empty());
    let with = l.load("with.xml").unwrap();
    let names: Vec<_> = with.children.iter().map(|c| c.name()).collect();
    assert_eq!(names, ["kid", "extra"]);
    assert!(with.component("Thing").is_some());
}

#[test]
fn errors_are_reported() {
    let mut l = loader(&[
        ("a.xml", r#"<Entity><Base file="b.xml"/></Entity>"#),
        ("b.xml", r#"<Entity><Base file="a.xml"/></Entity>"#),
        ("sprite.xml", r#"<Sprite/>"#),
        ("broken.xml", r#"<Entity><Thing a="1></Entity>"#),
    ]);
    assert!(matches!(l.load("a.xml"), Err(LoadError::Cycle(_))));
    assert!(matches!(
        l.load("sprite.xml"),
        Err(LoadError::NotAnEntity(_))
    ));
    assert!(matches!(l.load("missing.xml"), Err(LoadError::Missing(_))));
    assert!(matches!(l.load("broken.xml"), Err(LoadError::Xml(..))));
}
