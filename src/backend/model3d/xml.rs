use super::Result;
use std::collections::BTreeMap;

const MAX_XML_BYTES: usize = 64 * 1024 * 1024;
const MAX_XML_NODES: usize = 1_000_000;
const MAX_XML_DEPTH: usize = 64;
const MAX_XML_ATTRIBUTES: usize = 64;
const MAX_XML_NAME_BYTES: usize = 256;
const MAX_ENTITY_BYTES: usize = 16;

pub(super) struct Node {
    pub name: String,
    pub attrs: BTreeMap<String,String>,
    pub text: String,
    pub children: Vec<usize>,
}

pub(super) struct Document { pub nodes: Vec<Node>, pub root: usize }

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
            .or_else(|| self.attrs.iter().find(|(k,_)| local(k) == name).map(|(_,v)| v.as_str()))
    }

    pub fn number(&self, name: &str) -> Result<f64> { super::number(self.attr(name).ok_or_else(|| format!("missing XML attribute {name}"))?) }
    pub fn index(&self, name: &str) -> Result<u32> { self.attr(name).ok_or("missing XML index")?.parse().map_err(|_| "invalid XML index".into()) }
}

impl Document {
    pub fn children<'a>(&'a self, node: usize, name: &'a str) -> impl Iterator<Item=usize> + 'a {
        self.nodes[node].children.iter().copied().filter(move |&i| self.nodes[i].name == name)
    }

    pub fn child(&self, node: usize, name: &str) -> Option<usize> { self.children(node,name).next() }

    pub fn value(&self, node: usize, name: &str) -> Result<f64> {
        super::number(self.nodes[self.child(node,name).ok_or("missing XML coordinate")?].text.trim())
    }
}

fn local(name: &str) -> &str { name.rsplit(':').next().unwrap_or(name) }

fn decoded(source: &str) -> Result<String> {
    let mut out = String::new();
    let mut left = source;
    while let Some(at) = left.find('&') {
        out.push_str(&left[..at]);
        left = &left[at+1..];
        let end = left.find(';').filter(|&i| i <= MAX_ENTITY_BYTES).ok_or("invalid XML entity")?;
        let entity = &left[..end];
        let c = match entity {
            "lt" => '<', "gt" => '>', "amp" => '&', "quot" => '"', "apos" => '\'',
            _ => {
                let n = if let Some(hex) = entity.strip_prefix("#x") { u32::from_str_radix(hex,16).ok() }
                    else { entity.strip_prefix('#').and_then(|n| n.parse().ok()) };
                n.and_then(char::from_u32).filter(|c| *c == '\t' || *c == '\n' || *c == '\r' || !c.is_control()).ok_or("unsupported XML entity")?
            }
        };
        out.push(c);
        left = &left[end+1..];
    }
    out.push_str(left);
    Ok(out)
}

fn name(source: &str) -> bool {
    !source.is_empty() && source.len() <= MAX_XML_NAME_BYTES && source.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c,b'_'|b'-'|b'.'|b':'))
}

// Sample input: <model unit="millimeter"><metadata name="Application">Slicer &amp; tools</metadata></model>.
pub(super) fn parse(source: &str) -> Result<Document> {
    if source.len() > MAX_XML_BYTES { return Err("XML exceeds its bound".into()) }
    if source.chars().any(|c| c.is_control() && !matches!(c,'\t'|'\r'|'\n')) { return Err("XML contains control characters".into()) }
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack: Vec<(usize,String)> = Vec::new();
    let mut root = None;
    let mut at = usize::from(source.starts_with('\u{feff}'))*3;
    while at < source.len() {
        let left = &source[at..];
        if !left.starts_with('<') {
            let end = left.find('<').unwrap_or(left.len());
            let value = decoded(&left[..end])?;
            if let Some(&(i,_)) = stack.last() { nodes[i].text.push_str(&value) }
            else if !value.trim().is_empty() { return Err("text outside XML root".into()) }
            at += end;
            continue;
        }
        if let Some(body) = left.strip_prefix("<!--") {
            let end = body.find("-->").ok_or("truncated XML comment")?;
            if body[..end].contains("--") { return Err("invalid XML comment".into()) }
            at += 4+end+3;
            continue;
        }
        if let Some(body) = left.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").ok_or("truncated XML CDATA")?;
            let i = stack.last().ok_or("CDATA outside XML root")?.0;
            nodes[i].text.push_str(&body[..end]);
            at += 9+end+3;
            continue;
        }
        if left.starts_with("<?") {
            let end = left.find("?>").ok_or("truncated XML declaration")?;
            at += end+2;
            continue;
        }
        if left.starts_with("<!") { return Err("XML declarations and external entities are refused".into()) }
        let mut quoted = None;
        let end = left.char_indices().skip(1).find_map(|(i,c)| {
            if quoted == Some(c) { quoted = None }
            else if quoted.is_none() && matches!(c,'\''|'"') { quoted = Some(c) }
            else if quoted.is_none() && c == '>' { return Some(i) }
            None
        }).ok_or("truncated XML tag")?;
        let tag = left[1..end].trim();
        at += end+1;
        if let Some(close) = tag.strip_prefix('/') {
            let (_,opened) = stack.pop().ok_or("unexpected XML closing tag")?;
            if close.trim() != opened { return Err("mismatched XML closing tag".into()) }
            continue;
        }
        let empty = tag.ends_with('/');
        let tag = if empty { tag[..tag.len()-1].trim_end() } else { tag };
        let split = tag.find(char::is_whitespace).unwrap_or(tag.len());
        let tag_name = &tag[..split];
        if !name(tag_name) { return Err("invalid XML tag name".into()) }
        let mut attrs = BTreeMap::new();
        let mut rest = tag[split..].trim_start();
        while !rest.is_empty() {
            let equals = rest.find('=').ok_or("missing XML attribute value")?;
            let key = rest[..equals].trim_end();
            if attrs.len() >= MAX_XML_ATTRIBUTES { return Err("XML attribute count exceeds its bound".into()) }
            if !name(key) { return Err("invalid XML attribute".into()) }
            rest = rest[equals+1..].trim_start();
            let quote = rest.chars().next().filter(|c| matches!(c,'\''|'"')).ok_or("unquoted XML attribute")?;
            rest = &rest[1..];
            let end = rest.find(quote).ok_or("truncated XML attribute")?;
            if rest[..end].contains('<') { return Err("invalid XML attribute contents".into()) }
            if attrs.insert(key.into(),decoded(&rest[..end])?).is_some() { return Err("duplicate XML attribute".into()) }
            rest = &rest[end+1..];
            if !rest.is_empty() && !rest.starts_with(char::is_whitespace) { return Err("missing XML attribute separator".into()) }
            rest = rest.trim_start();
        }
        if nodes.len() >= MAX_XML_NODES || stack.len() >= MAX_XML_DEPTH { return Err("XML structure exceeds its bound".into()) }
        let i = nodes.len();
        nodes.push(Node { name: local(tag_name).into(), attrs, text: String::new(), children: Vec::new() });
        if let Some(&(parent,_)) = stack.last() { nodes[parent].children.push(i) }
        else if root.replace(i).is_some() { return Err("multiple XML roots".into()) }
        if !empty { stack.push((i,tag_name.into())) }
    }
    if !stack.is_empty() { return Err("truncated XML document".into()) }
    Ok(Document { nodes, root: root.ok_or("empty XML document")? })
}
