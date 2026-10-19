use super::limits::*;
use super::{Layer, Model, Result, Segment};
use std::collections::BTreeMap;

const LAYER_EPSILON_MM: f64 = 0.0001;
const ARC_STEP_RADIANS: f64 = 0.08726646;
const MAX_ARC_SEGMENTS: usize = 16_384;
const MAX_TOOLS: f64 = 256.;

struct Machine {
    position: [f64;3],
    offset: [f64;3],
    extruders: BTreeMap<u16,f64>,
    tool: u16,
    relative: bool,
    relative_e: bool,
    arc_absolute: bool,
    plane: [usize;3],
    unit: f64,
    feed: f64,
    motion: Option<u32>,
    last_extrusion: usize,
    arc_layer: bool,
}

impl Default for Machine {
    fn default() -> Self {
        Self { position: [0.;3], offset: [0.;3], extruders: BTreeMap::new(), tool: 0, relative: false,
            relative_e: false, arc_absolute: false, plane: [0,1,2], unit: 1., feed: 0., motion: None, last_extrusion: 0, arc_layer: false }
    }
}

// Sample input: G1 X10.5 Y-2 E0.3 F1200 (move)*42.
fn words(source: &str) -> Result<BTreeMap<u8,f64>> {
    let source = source.split('*').next().unwrap_or("");
    let bytes = source.as_bytes();
    let mut at = 0;
    let mut out = BTreeMap::new();
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() { at += 1; continue }
        if bytes[at] == b'(' {
            at += bytes[at..].iter().position(|&v| v == b')').ok_or("truncated G-code comment")?+1;
            continue;
        }
        let key = bytes[at].to_ascii_uppercase();
        if !key.is_ascii_alphabetic() { return Err("invalid G-code word".into()) }
        at += 1;
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) { at += 1 }
        let begin = at;
        if matches!(bytes.get(at),Some(b'+'|b'-')) { at += 1 }
        while bytes.get(at).is_some_and(|b| b.is_ascii_digit() || *b == b'.') { at += 1 }
        if at == begin { return Err("G-code word has no value".into()) }
        let value = super::number(&source[begin..at])?;
        if out.insert(key,value).is_some() { return Err("duplicate G-code word".into()) }
    }
    Ok(out)
}

// Sample input: N42 G1 X10*52.
fn command_source(source: &str) -> Result<&str> {
    let source = source.split('*').next().unwrap_or("").trim();
    if !source.starts_with(['N','n']) { return Ok(source) }
    let source = source[1..].trim_start();
    let end = source.bytes().take_while(u8::is_ascii_digit).count();
    if end == 0 { return Err("invalid G-code line number".into()) }
    super::number(&source[..end])?;
    Ok(source[end..].trim_start())
}

impl Machine {
    fn layer(&self, model: &mut Model, z: f64) -> Result<()> {
        if model.layers.last().is_some_and(|l| (f64::from(l.z_mm)-z).abs() <= LAYER_EPSILON_MM) { return Ok(()) }
        if model.layers.len() >= MAX_LAYERS { return Err("too many toolpath layers".into()) }
        let first = self.last_extrusion;
        if let Some(layer) = model.layers.last_mut() { layer.segment_count = (first-layer.first_segment as usize) as u32 }
        model.layers.push(Layer { z_mm: z as f32, first_segment: first as u32, segment_count: 0 });
        Ok(())
    }

    fn segment(&mut self, model: &mut Model, end: [f64;3], extrusion: f64) -> Result<()> {
        if model.segments.len() >= super::MAX_SEGMENTS || end.iter().any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE)
            || !extrusion.is_finite() || extrusion.abs() > MAX_COORDINATE || !self.feed.is_finite() || self.feed > MAX_COORDINATE {
            return Err("G-code geometry exceeds its bound".into());
        }
        if end == self.position { return Ok(()) }
        if extrusion > 0. && !self.arc_layer { self.layer(model,end[2])? }
        model.segments.push(Segment { start: self.position.map(|v| v as f32), end: end.map(|v| v as f32),
            extrusion_mm: extrusion as f32, feed_mm_min: self.feed as f32, tool: self.tool });
        self.position = end;
        if extrusion > 0. { self.last_extrusion = model.segments.len() }
        Ok(())
    }

    fn arc(&mut self, model: &mut Model, end: [f64;3], extrusion: f64, w: &BTreeMap<u8,f64>, clockwise: bool) -> Result<()> {
        let [a,b,c] = self.plane;
        let start = self.position;
        let keys = b"IJK";
        let center = if let Some(&r) = w.get(&b'R') {
            let radius = r.abs()*self.unit;
            let dx = end[a]-start[a];
            let dy = end[b]-start[b];
            let chord = dx.hypot(dy);
            if chord == 0. || chord > radius*2. { return Err("invalid G-code arc radius".into()) }
            let height = (radius*radius-chord*chord/4.).max(0.).sqrt();
            let sign = if clockwise == (r > 0.) { -1. } else { 1. };
            [(start[a]+end[a])/2.-sign*dy*height/chord,(start[b]+end[b])/2.+sign*dx*height/chord]
        } else {
            if !w.contains_key(&keys[a]) && !w.contains_key(&keys[b]) { return Err("G-code arc has no center".into()) }
            [a,b].map(|axis| if self.arc_absolute { w.get(&keys[axis]).map_or(start[axis],|v| v*self.unit+self.offset[axis]) }
                else { start[axis]+w.get(&keys[axis]).copied().unwrap_or(0.)*self.unit })
        };
        let radius = (start[a]-center[0]).hypot(start[b]-center[1]);
        let end_radius = (end[a]-center[0]).hypot(end[b]-center[1]);
        if radius <= 0. || (radius-end_radius).abs() > 0.05f64.max(radius*0.001) { return Err("inconsistent G-code arc".into()) }
        let angle = (start[b]-center[1]).atan2(start[a]-center[0]);
        let mut sweep = ((end[b]-center[1]).atan2(end[a]-center[0])-angle).rem_euclid(std::f64::consts::TAU);
        if clockwise { sweep -= std::f64::consts::TAU } else if sweep == 0. { sweep = std::f64::consts::TAU }
        let count = (sweep.abs()*radius).max(sweep.abs()/ARC_STEP_RADIANS).ceil().max(1.) as usize;
        if count > MAX_ARC_SEGMENTS || count > super::MAX_SEGMENTS-model.segments.len() { return Err("G-code arc exceeds its bound".into()) }
        if extrusion > 0. { self.layer(model,end[2])? }
        self.arc_layer = true;
        for i in 1..=count {
            let t = i as f64/count as f64;
            let mut point = end;
            point[a] = center[0]+radius*(angle+sweep*t).cos();
            point[b] = center[1]+radius*(angle+sweep*t).sin();
            point[c] = start[c]+(end[c]-start[c])*t;
            self.segment(model,if i == count { end } else { point },extrusion/count as f64)?;
        }
        self.arc_layer = false;
        Ok(())
    }

    fn line(&mut self, model: &mut Model, source: &str) -> Result<()> {
        let source = command_source(source)?;
        if source.is_empty() || source == "%" { return Ok(()) }
        // Display strings and vendor macros are not motion and need no numeric interpretation.
        if source.starts_with(['M','m']) {
            let command = source[1..].trim_start();
            let end = command.bytes().take_while(|b| b.is_ascii_digit() || *b == b'.').count();
            match super::number(&command[..end]).ok() {
                Some(82.) => self.relative_e = false,
                Some(83.) => self.relative_e = true,
                _ => {},
            }
            return Ok(());
        }
        if !source.as_bytes().first().is_some_and(|b| matches!(b.to_ascii_uppercase(),b'G'|b'M'|b'N'|b'T'|b'X'|b'Y'|b'Z'|b'E'|b'F')) { return Ok(()) }
        let arguments = source[1..].trim_start();
        let command = source.split_whitespace().next().unwrap_or("");
        if command.contains('_') && arguments.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
            && command.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') { return Ok(()) }
        if source.starts_with(['G','g']) {
            let end = arguments.bytes().take_while(|b| b.is_ascii_digit() || *b == b'.').count();
            if let Ok(g) = super::number(&arguments[..end]) {
                if !matches!(g,0.|1.|2.|3.|17.|18.|19.|20.|21.|90.|91.|90.1|91.1|92.) { return Ok(()) }
            }
        }
        let w = words(source)?;
        if let Some(&tool) = w.get(&b'T') {
            if tool < 0. || tool.fract() != 0. || tool >= MAX_TOOLS { return Err("invalid G-code tool".into()) }
            self.tool = tool as u16;
        }
        if w.get(&b'M') == Some(&82.) { self.relative_e = false; return Ok(()) }
        if w.get(&b'M') == Some(&83.) { self.relative_e = true; return Ok(()) }
        if let Some(&g) = w.get(&b'G') {
            match g {
                0. | 1. | 2. | 3. => self.motion = Some(g as u32),
                17. => { self.plane = [0,1,2]; return Ok(()) },
                18. => { self.plane = [2,0,1]; return Ok(()) },
                19. => { self.plane = [1,2,0]; return Ok(()) },
                20. => { self.unit = 25.4; return Ok(()) },
                21. => { self.unit = 1.; return Ok(()) },
                90. => { self.relative = false; self.relative_e = false; return Ok(()) },
                91. => { self.relative = true; self.relative_e = true; return Ok(()) },
                90.1 => { self.arc_absolute = true; return Ok(()) },
                91.1 => { self.arc_absolute = false; return Ok(()) },
                92. => {
                    for (axis,key) in b"XYZ".iter().enumerate() { if let Some(v) = w.get(key) { self.offset[axis] = self.position[axis]-v*self.unit } }
                    if let Some(v) = w.get(&b'E') { self.extruders.insert(self.tool,v*self.unit); }
                    return Ok(());
                }
                _ => return Ok(()),
            }
        }
        if let Some(&feed) = w.get(&b'F') { if feed <= 0. { return Err("invalid G-code feed".into()) } self.feed = feed*self.unit }
        if !w.contains_key(&b'G') && !b"XYZEIJKR".iter().any(|key| w.contains_key(key)) { return Ok(()) }
        let Some(motion) = self.motion else { return Ok(()) };
        let mut end = self.position;
        for (axis,key) in b"XYZ".iter().enumerate() {
            if let Some(v) = w.get(key) { end[axis] = v*self.unit + if self.relative { self.position[axis] } else { self.offset[axis] } }
        }
        let mut extrusion = 0.;
        if let Some(v) = w.get(&b'E') {
            let old = self.extruders.entry(self.tool).or_default();
            let next = v*self.unit + if self.relative_e { *old } else { 0. };
            extrusion = next-*old;
            *old = next;
        }
        if matches!(motion,2|3) { self.arc(model,end,extrusion,&w,motion == 2) } else { self.segment(model,end,extrusion) }
    }
}

// Sample input: ; printer_model = MK4\nG90\nM83\nG1 X10 E1 F1200\n.
pub(super) fn parse(bytes: &[u8], model: &mut Model) -> Result<()> {
    let source = super::text(bytes)?;
    let mut machine = Machine::default();
    let mut thumbnail: Option<(String,usize,String)> = None;
    for line in source.lines() {
        if line.len() > MAX_LINE_BYTES { return Err("G-code line exceeds its bound".into()) }
        let (code,comment) = line.split_once(';').unwrap_or((line,""));
        let comment = comment.trim();
        if let Some((format,expected,encoded)) = thumbnail.as_mut() {
            if comment.starts_with("thumbnail") && comment.contains(" end") {
                if encoded.len() != *expected { return Err("G-code thumbnail length mismatch".into()) }
                let bytes = super::codecs::base64(encoded)?;
                model.thumbnail(None,format,bytes)?;
                thumbnail = None;
            } else {
                if !code.trim().is_empty() || encoded.len()+comment.len() > super::MAX_THUMBNAIL*2 { return Err("invalid G-code thumbnail".into()) }
                encoded.push_str(comment);
            }
            continue;
        }
        if comment.starts_with("thumbnail") && comment.contains(" begin ") {
            let w: Vec<&str> = comment.split_whitespace().collect();
            if w.len() != 4 { return Err("invalid G-code thumbnail header".into()) }
            let expected = w[3].parse::<usize>().map_err(|_| "invalid thumbnail length")?;
            if expected > super::MAX_THUMBNAIL*2 { return Err("G-code thumbnail exceeds its bound".into()) }
            let format = match w[0] { "thumbnail" => "png", "thumbnail_JPG" => "jpg", "thumbnail_QOI" => "qoi", _ => return Err("unsupported thumbnail format".into()) };
            thumbnail = Some((format.into(),expected,String::new()));
            continue;
        }
        super::projectmeta::pairs(model,comment);
        machine.line(model,code)?;
    }
    if thumbnail.is_some() { return Err("truncated G-code thumbnail".into()) }
    if let Some(layer) = model.layers.last_mut() { layer.segment_count = model.segments.len() as u32-layer.first_segment }
    else if !model.segments.is_empty() { model.layers.push(Layer { z_mm: machine.position[2] as f32, first_segment: 0, segment_count: model.segments.len() as u32 }) }
    Ok(())
}

