//! `.FP1` recipe files.
//!
//! An FP1 is a small XML file written by Fujifilm's X RAW Studio:
//!
//! ```xml
//! <ConversionProfile application="XRFC" version="1.11.0.0">
//!     <PropertyGroup device="X-T3" version="X-T3_0100" label="Kodak Tri-X">
//!         <FilmSimulation>Acros</FilmSimulation>
//!         ...
//! ```
//!
//! X Recipe Bench keeps a recipe's description inside the same file, as an XML
//! comment directly after the opening tag. Comments are ignored by anything
//! else that reads the file. Editing a description changes only that comment;
//! every other byte of the file is left as it was.

const DESCRIPTION_TAG: &str = "X Recipe Bench description:";
/// The marker used while the app was called X Raw Probe. Still read, so
/// descriptions written then are not lost; rewritten with the new marker
/// the next time the description is saved.
const OLD_DESCRIPTION_TAG: &str = "X Raw Probe description:";

/// The description inside a comment's text, if the comment is one of ours.
fn description_in(comment: &str) -> Option<&str> {
    let comment = comment.trim();
    comment.strip_prefix(DESCRIPTION_TAG).or_else(|| comment.strip_prefix(OLD_DESCRIPTION_TAG))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    /// The name shown in lists (the file's `label`).
    pub name: String,
    /// Camera model the recipe was made for, e.g. "X-T3". May be empty.
    pub device: String,
    /// The file's `version` attribute, e.g. "X-T3_0100". May be empty.
    pub device_version: String,
    pub description: String,
    /// Every setting in the file, in file order: (tag, text).
    pub fields: Vec<(String, String)>,
    /// The file's text exactly as read, so edits can preserve it.
    source: String,
}

impl Recipe {
    pub fn parse(text: &str) -> Result<Recipe, String> {
        let text = text.trim_start_matches('\u{feff}');
        // craft-xml reads the file. Comments are kept: the description lives in one.
        let options = craft_xml::Options { comments: true, ..Default::default() };
        let root = options.parse(text.as_bytes()).map_err(|e| format!("not a readable recipe file ({e})"))?;
        if root.local() != "ConversionProfile" {
            return Err("not an FP1 recipe (no ConversionProfile)".into());
        }
        let group = root.child("PropertyGroup").ok_or("not an FP1 recipe (no PropertyGroup)")?;

        let fields = group.elements().map(|n| (n.local().to_string(), n.text().trim().to_string())).collect();

        let description = root
            .children
            .iter()
            .filter_map(|n| match n {
                craft_xml::Node::Comment(c) => Some(c.as_str()),
                _ => None,
            })
            .find_map(|t| description_in(t).map(decode_comment))
            .unwrap_or_default();

        Ok(Recipe {
            name: group.attr("label").unwrap_or("").to_string(),
            device: group.attr("device").unwrap_or("").to_string(),
            device_version: group.attr("version").unwrap_or("").to_string(),
            description,
            fields,
            source: text.to_string(),
        })
    }

    /// The text of the first field with this tag, if it is present and not empty.
    pub fn field(&self, tag: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(t, _)| t == tag)
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.is_empty())
    }

    /// The file's text with the description replaced. Nothing else changes.
    pub fn with_description(&self, description: &str) -> String {
        let without = remove_description_comment(&self.source);
        let description = description.trim();
        if description.is_empty() {
            return without;
        }
        // Insert after the end of the <ConversionProfile ...> opening tag.
        let Some(start) = without.find("<ConversionProfile") else { return without };
        let Some(end) = without[start..].find('>').map(|i| start + i + 1) else { return without };
        let newline = if without.contains("\r\n") { "\r\n" } else { "\n" };
        format!(
            "{}{newline}    <!-- {DESCRIPTION_TAG} {} -->{}",
            &without[..end],
            encode_comment(description),
            &without[end..]
        )
    }

    /// This recipe's file text with some settings replaced. `changes` are
    /// (tag, text) pairs; tags the file lacks are added at the end.
    pub fn with_fields(&self, changes: &[(String, String)]) -> String {
        let mut fields = self.fields.clone();
        for (tag, value) in changes {
            match fields.iter_mut().find(|(t, _)| t == tag) {
                Some(field) => field.1 = value.clone(),
                None => fields.push((tag.clone(), value.clone())),
            }
        }
        let mut text = Recipe::build(&self.name, &self.device, &self.description, &fields);
        if !self.device_version.is_empty() {
            let default_version = format!("version=\"{}_0100\"", escape(&self.device));
            let real_version = format!("version=\"{}\"", escape(&self.device_version));
            text = text.replacen(&default_version, &real_version, 1);
        }
        text
    }

    /// This recipe's file text under another name.
    pub fn renamed(&self, name: &str) -> String {
        let mut copy = self.clone();
        copy.name = name.to_string();
        copy.with_fields(&[])
    }

    /// Builds a new recipe file from scratch.
    pub fn build(name: &str, device: &str, description: &str, fields: &[(String, String)]) -> String {
        let mut out = String::new();
        out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
        out.push_str("<ConversionProfile application=\"XRFC\" version=\"1.11.0.0\">\n");
        if !description.trim().is_empty() {
            out.push_str(&format!(
                "    <!-- {DESCRIPTION_TAG} {} -->\n",
                encode_comment(description.trim())
            ));
        }
        out.push_str(&format!(
            "    <PropertyGroup device=\"{}\" version=\"{}_0100\" label=\"{}\">\n",
            escape(device),
            escape(device),
            escape(name)
        ));
        for (tag, value) in fields {
            if value.is_empty() {
                out.push_str(&format!("        <{tag}/>\n"));
            } else {
                out.push_str(&format!("        <{tag}>{}</{tag}>\n", escape(value)));
            }
        }
        out.push_str("    </PropertyGroup>\n</ConversionProfile>\n");
        out
    }
}

fn remove_description_comment(text: &str) -> String {
    let mut search_from = 0;
    while let Some(open) = text[search_from..].find("<!--").map(|i| search_from + i) {
        let Some(close) = text[open..].find("-->").map(|i| open + i + 3) else { break };
        if description_in(&text[open + 4..close - 3]).is_some() {
            // Also remove the line break and indentation that came before it.
            let before = text[..open].trim_end_matches([' ', '\t']);
            let before = before.strip_suffix("\r\n").or_else(|| before.strip_suffix('\n')).unwrap_or(&text[..open]);
            return format!("{before}{}", &text[close..]);
        }
        search_from = close;
    }
    text.to_string()
}

/// XML comments may not contain "--", so it is stored with a marker between.
fn encode_comment(text: &str) -> String {
    let mut out = text.replace("--", "-\u{200b}-");
    while out.contains("--") {
        out = out.replace("--", "-\u{200b}-");
    }
    out
}

fn decode_comment(text: &str) -> String {
    text.trim().replace('\u{200b}', "")
}

/// Text made safe to put inside an element or an attribute (craft-xml's escaping).
fn escape(text: &str) -> String {
    craft_xml::esc_attr(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ConversionProfile application="XRFC" version="1.11.0.0">
    <PropertyGroup device="X-T3" version="X-T3_0100" label="Push Process">
        <SerialNumber></SerialNumber>
        <IOPCode>FF159501</IOPCode>
        <ExposureBias>P0P33</ExposureBias>
        <FilmSimulation>Acros</FilmSimulation>
        <ColorChromeBlue/>
        <HighlightTone>3</HighlightTone>
    </PropertyGroup>
</ConversionProfile>
"#;

    #[test]
    fn reads_name_device_and_fields() {
        let r = Recipe::parse(SAMPLE).unwrap();
        assert_eq!(r.name, "Push Process");
        assert_eq!(r.device, "X-T3");
        assert_eq!(r.field("FilmSimulation"), Some("Acros"));
        assert_eq!(r.field("ColorChromeBlue"), None, "an empty tag means 'not set'");
        assert_eq!(r.description, "");
    }

    #[test]
    fn description_round_trips_and_leaves_the_rest_alone() {
        let r = Recipe::parse(SAMPLE).unwrap();
        let text = r.with_description("Gritty black and white -- pushed two stops.\nGood at night.");
        let again = Recipe::parse(&text).unwrap();
        assert_eq!(again.description, "Gritty black and white -- pushed two stops.\nGood at night.");
        assert_eq!(again.fields, r.fields);
        // Clearing the description gives back the original file exactly.
        assert_eq!(again.with_description(""), SAMPLE);
        // Replacing does not pile up comments.
        let replaced = again.with_description("Second version");
        assert_eq!(replaced.matches("<!--").count(), 1);
        assert_eq!(Recipe::parse(&replaced).unwrap().description, "Second version");
    }

    #[test]
    fn descriptions_written_under_the_old_app_name_are_still_read() {
        let old = SAMPLE.replace(
            "<PropertyGroup",
            "<!-- X Raw Probe description: From the old days. -->\n    <PropertyGroup",
        );
        let r = Recipe::parse(&old).unwrap();
        assert_eq!(r.description, "From the old days.");
        // Saving replaces the old marker; it does not leave two comments behind.
        let saved = r.with_description("Updated.");
        assert_eq!(saved.matches("<!--").count(), 1);
        assert!(saved.contains("X Recipe Bench description: Updated."));
        assert_eq!(Recipe::parse(&saved).unwrap().description, "Updated.");
    }

    #[test]
    fn built_files_parse_back() {
        let fields = vec![
            ("FilmSimulation".to_string(), "Classic".to_string()),
            ("SmoothSkinEffect".to_string(), String::new()),
        ];
        let text = Recipe::build("Tom & Jerry \"1\"", "X-T4", "A <test>", &fields);
        let r = Recipe::parse(&text).unwrap();
        assert_eq!(r.name, "Tom & Jerry \"1\"");
        assert_eq!(r.device, "X-T4");
        assert_eq!(r.description, "A <test>");
        assert_eq!(r.fields, fields);
    }

    #[test]
    fn rejects_other_files() {
        assert!(Recipe::parse("hello").is_err());
        assert!(Recipe::parse("<Other/>").is_err());
    }
}
