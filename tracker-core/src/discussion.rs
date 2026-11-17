use crate::github::SharedGitHub;
use anyhow::{Result, bail};
use chrono::Utc;
use serde_json::{Value, json};
use std::path::Path;

/// Cache each page independently. A failed refresh leaves the last readable copy intact.
pub async fn load(directory: &Path, item: &Value, page: u64, force: bool, github: Option<SharedGitHub>) -> Result<Value> {
    if page == 0 { bail!("Invalid discussion page"); }
    let repo = item["repo"].as_str().unwrap_or_default();
    let number = item["number"].as_u64().unwrap_or_default();
    let normalized = crate::repository_config::normalize_name(repo)?;
    let mut folder = directory.join("discussions");
    // Prefix components so even dot-only names cannot become relative path segments.
    for component in normalized.split('/') { folder = folder.join(format!("repo-{component}")); }
    let path = folder.join(format!("{number}-{page}-reviews-v1.json"));
    let cached = std::fs::read(&path).ok().and_then(|raw| serde_json::from_slice::<Value>(&raw).ok());
    if !force && let Some(cache) = &cached {
        if cache["version"] == item["updated_at"] && Utc::now().timestamp() - cache["fetched"].as_i64().unwrap_or_default() < 300 {
            return Ok(cache["data"].clone());
        }
    }
    let result = if let Some(github) = github {
        async {
            let mut client=github.0.lock().await;
            let mut comments=client.discussion(repo,number,page).await?;
            for comment in &mut comments.items {comment["kind"]=json!("comment");}
            if item["type"]=="pr" {
                let reviews=client.review_summaries(repo,number,page).await?;
                if reviews.next.is_some(){comments.next=reviews.next;}
                for mut review in reviews.items {if review["state"]=="PENDING" {continue;}review["kind"]=json!("review");review["created_at"]=review["submitted_at"].clone();review["updated_at"]=review["submitted_at"].clone();comments.items.push(review);}
            }
            Ok::<_,anyhow::Error>(comments)
        }.await
    } else {
        Err(anyhow::anyhow!("Offline · provide a GitHub token, then reopen to load comments"))
    };
    match result {
        Ok(response) => {
            let comments: Vec<Value> = response.items.iter().map(|comment|json!({
                "id":comment["id"], "kind":comment["kind"], "review_state":comment["state"].as_str().unwrap_or(""), "author":comment["user"]["login"].as_str().unwrap_or("Unknown author"),
                "body":comment["body"].as_str().unwrap_or(""), "created_at":comment["created_at"],
                "updated_at":comment["updated_at"], "url":comment["html_url"],
            })).collect();
            let mut data = json!({"comments":comments,"next":response.next,"warning":null});
            let cache = json!({"version":item["updated_at"],"fetched":Utc::now().timestamp(),"data":data});
            if let Err(error) = (|| -> Result<()> {
                std::fs::create_dir_all(path.parent().unwrap())?;
                let temp = path.with_extension("json.new");
                std::fs::write(&temp, serde_json::to_vec(&cache)?)?;
                std::fs::rename(temp, &path)?;
                Ok(())
            })() { data["warning"] = json!(format!("Comments loaded, but could not cache them: {error}")); }
            Ok(data)
        }
        Err(error) => {
            if let Some(cache) = cached {
                let mut data = cache["data"].clone();
                data["warning"] = json!(format!("Showing saved comments · {error}"));
                Ok(data)
            } else { Err(error) }
        }
    }
}

/// Readable comment text for the compact native discussion view. HTML is never executed.
/// Parse Markdown before removing HTML so literal code such as Vec<T> remains intact.
pub fn display_body(body: &str) -> String {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    fn append(body: &str, out: &mut String, allow_html: bool) {
        use std::sync::OnceLock;
        static TAGS:OnceLock<regex::Regex>=OnceLock::new();
        static COMMENTS:OnceLock<regex::Regex>=OnceLock::new();
        let mut links=Vec::new();
        for event in Parser::new(body) {
            match event {
                Event::Start(Tag::Link{dest_url,..})=>links.push(dest_url.into_string()),
                Event::End(TagEnd::Link)=>{if let Some(url)=links.pop(){out.push_str(" (");out.push_str(&url);out.push(')');}},
                Event::Text(s)|Event::Code(s)=>out.push_str(&s),
                Event::SoftBreak|Event::HardBreak|Event::Rule=>out.push('\n'),
                Event::Start(Tag::Item)=>out.push_str("• "),
                Event::End(TagEnd::Paragraph|TagEnd::Heading(_)|TagEnd::Item|TagEnd::CodeBlock)=>out.push('\n'),
                Event::Html(s)|Event::InlineHtml(s) if allow_html=>{
                    let comments=COMMENTS.get_or_init(||regex::Regex::new(r"(?s)<!--.*?-->").unwrap());
                    let tags=TAGS.get_or_init(||regex::Regex::new(r"(?s)</?[A-Za-z][^>]*>").unwrap());
                    let cleaned=comments.replace_all(&s,"");
                    let cleaned=tags.replace_all(&cleaned,"\n");
                    append(&cleaned,out,false);
                },
                _=>{}
            }
        }
    }
    let mut result=String::new();append(body,&mut result,true);
    result.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim().to_owned()
}
