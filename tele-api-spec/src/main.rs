mod openapi;

use std::{fs, path::PathBuf};

use serde::Serialize;
use structopt::StructOpt;
use tele_api_parser::BOT_API_DOCS_URL;

fn md_to_html(md: &str) -> String {
    let parser = pulldown_cmark::Parser::new(md);
    let mut buf = String::new();
    pulldown_cmark::html::push_html(&mut buf, parser);
    buf
}

struct Serialized {
    content: String,
    path:    String,
}

#[derive(Default)]
struct Indexer {
    publish_dir: PathBuf,
    inner:       Vec<Serialized>,
}

impl Indexer {
    fn new(publish_dir: &str) -> Self {
        Self {
            publish_dir: PathBuf::from(publish_dir),
            inner:       vec![],
        }
    }

    fn add<T: Serialize>(&mut self, api: &T, formats: Vec<Format>) -> anyhow::Result<()> {
        for format in formats {
            let (path, content) = match format {
                Format::Json(path) => (path, serde_json::to_string_pretty(api)?),
                Format::MinimizedJson(path) => (path, serde_json::to_string(api)?),
                Format::Yaml(path) => (path, serde_yml::to_string(api)?),
            };
            self.inner.push(Serialized {
                content,
                path: path.to_string(),
            });
        }

        Ok(())
    }

    fn generate(self) -> anyhow::Result<()> {
        if !self.publish_dir.exists() {
            fs::create_dir_all(&self.publish_dir)?;
        }

        let mut index = String::new();

        for Serialized { content, path } in self.inner {
            fs::write(self.publish_dir.join(&path), content)?;
            index += &format!("* [{path}]({path})\n");
        }

        let html = md_to_html(&index);
        fs::write(self.publish_dir.join("index.html"), html)?;

        Ok(())
    }
}

enum Format {
    Json(&'static str),
    MinimizedJson(&'static str),
    Yaml(&'static str),
}

#[derive(StructOpt)]
enum Path {
    /// Write files to `public` directory
    Production,
    /// Write files to `public/dev` directory
    Dev,
}

impl Path {
    const fn into_str(self) -> &'static str {
        match self {
            Self::Dev => "public/dev",
            Self::Production => "public",
        }
    }
}

fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let path = Path::from_args();

    let api = reqwest::blocking::get(BOT_API_DOCS_URL)?.text()?;
    let parsed = tele_api_parser::get(&api)?;

    let mut indexer = Indexer::new(path.into_str());
    let api = openapi::generate(parsed);
    indexer.add(
        &api,
        vec![
            Format::Json("openapi.json"),
            Format::MinimizedJson("openapi.min.json"),
            Format::Yaml("openapi.yaml"),
        ],
    )?;

    indexer.generate()?;

    Ok(())
}
