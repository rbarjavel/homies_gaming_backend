use crate::state::{MediaInfo, MediaType};
use askama::Template;

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate;

#[derive(Template)]
#[template(path = "media_container.html")]
pub struct MediaContainerTemplate;

#[derive(Template)]
#[template(path = "media_content.html")]
pub struct MediaContentTemplate<'a> {
    pub media_info: Option<&'a MediaInfo>,
}

#[derive(Template)]
#[template(path = "upload.html")]
pub struct UploadTemplate;

#[derive(Template)]
#[template(path = "greet.html")]
pub struct GreetTemplate {
    pub name: String,
}
