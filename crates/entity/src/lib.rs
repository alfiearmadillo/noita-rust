//! Entity files. An entity XML is an `<Entity>` with components (`<XComponent attr="..">` with
//! optional nested element trees), child `<Entity>` elements, an optional `<_Transform>` and
//! `<Base file="..">` elements that pull in another entity file and override parts of it.
//!
//! The merge rules are the documented modding behavior, not a verified port of the loader
//! (`0x575270` handles `<Base>`): inside `<Base>` a component element overrides the matching
//! component of the base (the k-th element of a type overrides the k-th component of that type;
//! attributes replace, nested elements merge by tag), and `<Entity>` elements inside it are only
//! added when `include_children` is set. Attribute values stay raw strings; `$[a-b]` ranges and
//! typed conversion belong to instantiation.

mod xml;

pub mod schema;
pub mod world;

use std::collections::HashMap;

/// A parsed element: tag, attributes in document order, children.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Node {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn set_attr(&mut self, name: &str, value: &str) {
        match self.attrs.iter_mut().find(|(k, _)| k == name) {
            Some((_, v)) => *v = value.to_owned(),
            None => self.attrs.push((name.to_owned(), value.to_owned())),
        }
    }

    pub fn child(&self, tag: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.tag == tag)
    }

    /// Applies `over` on top of `self`: its attributes replace, its child elements merge into the
    /// first child with the same tag that was not already matched, or are appended.
    pub fn merge(&mut self, over: &Node) {
        for (k, v) in &over.attrs {
            self.set_attr(k, v);
        }
        let mut used: Vec<usize> = Vec::new();
        for oc in &over.children {
            let slot = self
                .children
                .iter()
                .enumerate()
                .find(|(i, c)| c.tag == oc.tag && !used.contains(i))
                .map(|(i, _)| i);
            match slot {
                Some(i) => {
                    self.children[i].merge(oc);
                    used.push(i);
                }
                None => {
                    self.children.push(oc.clone());
                    used.push(self.children.len() - 1);
                }
            }
        }
    }
}

/// A resolved entity template.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct EntityDef {
    /// `name`, `tags` and any other attributes of the `<Entity>` element.
    pub attrs: Vec<(String, String)>,
    /// The `<_Transform>` element, if any (attributes like `position.x`).
    pub transform: Option<Node>,
    pub components: Vec<Node>,
    pub children: Vec<EntityDef>,
}

impl EntityDef {
    pub fn name(&self) -> &str {
        self.attr("name").unwrap_or("")
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The comma-separated `tags` attribute as a list.
    pub fn tags(&self) -> Vec<&str> {
        self.attr("tags")
            .map(|t| {
                t.split(',')
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn component(&self, type_name: &str) -> Option<&Node> {
        self.components.iter().find(|c| c.tag == type_name)
    }

    pub fn components_of<'a>(&'a self, type_name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.components.iter().filter(move |c| c.tag == type_name)
    }

    fn set_attr(&mut self, name: &str, value: &str) {
        match self.attrs.iter_mut().find(|(k, _)| k == name) {
            Some((_, v)) => *v = value.to_owned(),
            None => self.attrs.push((name.to_owned(), value.to_owned())),
        }
    }

    /// Own attributes override the base's, except `tags`, which accumulate.
    fn merge_attrs(&mut self, own: &[(String, String)]) {
        for (k, v) in own {
            if k == "tags" {
                let mut merged: Vec<String> = self
                    .attr("tags")
                    .map(|t| t.split(',').map(str::to_owned).collect())
                    .unwrap_or_default();
                for tag in v.split(',') {
                    if !tag.is_empty() && !merged.iter().any(|m| m == tag) {
                        merged.push(tag.to_owned());
                    }
                }
                self.set_attr("tags", &merged.join(","));
            } else {
                self.set_attr(k, v);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    Missing(String),
    Xml(String, String),
    NotAnEntity(String),
    /// A chain of `<Base>` files that includes itself.
    Cycle(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Missing(p) => write!(f, "{p}: file not found"),
            LoadError::Xml(p, e) => write!(f, "{p}: {e}"),
            LoadError::NotAnEntity(p) => write!(f, "{p}: root element is not <Entity>"),
            LoadError::Cycle(p) => write!(f, "{p}: <Base> cycle"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Reads game files by path (`data/entities/...`).
pub trait Files {
    fn read(&self, path: &str) -> Option<String>;
}

/// Files under a directory (the extracted game data root).
pub struct DiskFiles(pub std::path::PathBuf);

impl Files for DiskFiles {
    fn read(&self, path: &str) -> Option<String> {
        let rel = std::path::Path::new(path);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return None;
        }
        let bytes = std::fs::read(self.0.join(rel)).ok()?;
        // Some files start with a byte order mark.
        let text = String::from_utf8_lossy(&bytes);
        Some(text.trim_start_matches('\u{feff}').to_owned())
    }
}

/// Loads and caches resolved entity templates.
pub struct Loader<F: Files> {
    files: F,
    cache: HashMap<String, EntityDef>,
}

impl<F: Files> Loader<F> {
    pub fn new(files: F) -> Self {
        Self {
            files,
            cache: HashMap::new(),
        }
    }

    pub fn files(&self) -> &F {
        &self.files
    }

    /// The resolved template of an entity file.
    pub fn load(&mut self, path: &str) -> Result<EntityDef, LoadError> {
        self.load_chain(path, &mut Vec::new())
    }

    fn load_chain(&mut self, path: &str, chain: &mut Vec<String>) -> Result<EntityDef, LoadError> {
        if let Some(def) = self.cache.get(path) {
            return Ok(def.clone());
        }
        if chain.iter().any(|p| p == path) {
            return Err(LoadError::Cycle(path.to_owned()));
        }
        let text = self
            .files
            .read(path)
            .ok_or_else(|| LoadError::Missing(path.to_owned()))?;
        let root = xml::parse(&text).map_err(|e| LoadError::Xml(path.to_owned(), e))?;
        if root.tag != "Entity" {
            return Err(LoadError::NotAnEntity(path.to_owned()));
        }
        chain.push(path.to_owned());
        let result = self.resolve(&root, chain);
        chain.pop();
        let def = result?;
        self.cache.insert(path.to_owned(), def.clone());
        Ok(def)
    }

    fn resolve(&mut self, element: &Node, chain: &mut Vec<String>) -> Result<EntityDef, LoadError> {
        let mut def = EntityDef::default();
        let mut own_components = Vec::new();
        for child in &element.children {
            match child.tag.as_str() {
                "Base" => self.apply_base(&mut def, child, chain)?,
                "_Transform" => def.transform = Some(child.clone()),
                "Entity" => def.children.push(self.resolve(child, chain)?),
                _ => own_components.push(child.clone()),
            }
        }
        // The entity's own attributes apply on top of what its bases supplied.
        def.merge_attrs(&element.attrs);
        def.components.extend(own_components);
        Ok(def)
    }

    fn apply_base(
        &mut self,
        def: &mut EntityDef,
        base: &Node,
        chain: &mut Vec<String>,
    ) -> Result<(), LoadError> {
        let file = base.attr("file").unwrap_or("");
        let include_children = base
            .attr("include_children")
            .is_some_and(|v| v == "1" || v == "true");
        let loaded = self.load_chain(file, chain)?;
        def.merge_attrs(&loaded.attrs);
        if loaded.transform.is_some() {
            def.transform = loaded.transform.clone();
        }
        // Components of earlier bases stay in front.
        let first_new = def.components.len();
        def.components.extend(loaded.components.iter().cloned());
        if include_children {
            def.children.extend(loaded.children.iter().cloned());
        }
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for over in &base.children {
            let tag = over.tag.as_str();
            match tag {
                "_Transform" => match &mut def.transform {
                    Some(t) => t.merge(over),
                    None => def.transform = Some(over.clone()),
                },
                "Entity" => {
                    if include_children {
                        def.children.push(self.resolve(over, chain)?);
                    }
                }
                _ => {
                    let k = seen.entry(tag).or_insert(0);
                    let target = def.components[first_new..]
                        .iter_mut()
                        .filter(|c| c.tag == tag)
                        .nth(*k);
                    *k += 1;
                    match target {
                        Some(existing) => existing.merge(over),
                        None => def.components.push(over.clone()),
                    }
                }
            }
        }
        Ok(())
    }
}
