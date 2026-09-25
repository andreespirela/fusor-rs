//! Application transport and protocol. The standard library owns no endpoint,
//! serialization, HTTP status, domain validation or backend version policy.
use fusor_std::actions::{CancellationToken, Outcome};
use fusor_std::resources::fetch::get_text;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response};

#[derive(Clone, Debug, Deserialize)]
pub struct Project {
    pub id: u64,
    pub version: u64,
    pub title: String,
    pub body: String,
    pub seats: u32,
    pub last_operation: Option<String>,
}
#[derive(Serialize)]
pub struct UpdateProject {
    pub id: u64,
    pub expected_version: u64,
    pub title: String,
    pub body: String,
    pub seats: u32,
    pub operation: String,
}
#[derive(Debug, Deserialize)]
pub struct SaveError {
    pub message: String,
    pub field: Option<String>,
}
impl SaveError {
    fn transport(error: JsValue) -> Self {
        Self {
            message: error.as_string().unwrap_or_else(|| format!("{error:?}")),
            field: None,
        }
    }
}

pub async fn load(id: u64, cancel: CancellationToken) -> Result<Project, String> {
    let text = get_text(&format!("/editor/api/projects/{id}"), &cancel)
        .await
        .map_err(|error| error.to_string())?;
    let project: Project = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    if project.id != id {
        return Err("Unexpected project identity".into());
    }
    Ok(project)
}

pub async fn save(
    command: Rc<UpdateProject>,
    cancel: CancellationToken,
) -> Outcome<Project, SaveError> {
    async fn send(
        command: &UpdateProject,
        cancel: CancellationToken,
    ) -> Result<Outcome<Project, SaveError>, JsValue> {
        let options = RequestInit::new();
        options.set_method("POST");
        // Aborts the Fetch, including the body read, if the save is cancelled.
        options.set_signal(Some(&cancel.abort_signal()?));
        let body = serde_json::to_string(command)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        options.set_body(&JsValue::from_str(&body));
        let request = Request::new_with_str_and_init(
            &format!("/editor/api/projects/{}", command.id),
            &options,
        )?;
        request.headers().set("Content-Type", "application/json")?;
        let response: Response = JsFuture::from(
            web_sys::window()
                .ok_or_else(|| JsValue::from_str("No browser"))?
                .fetch_with_request(&request),
        )
        .await?
        .dyn_into()?;
        let status = response.status();
        let text = JsFuture::from(response.text()?)
            .await?
            .as_string()
            .ok_or_else(|| JsValue::from_str("Non-text response"))?;
        if status == 200 {
            let saved: Project = serde_json::from_str(&text)
                .map_err(|error| JsValue::from_str(&error.to_string()))?;
            if saved.id != command.id
                || saved.version <= command.expected_version
                || saved.last_operation.as_deref() != Some(command.operation.as_str())
            {
                return Err(JsValue::from_str("Unverified save response"));
            }
            Ok(Outcome::Accepted(saved))
        } else if matches!(status, 409 | 422) {
            let error = serde_json::from_str(&text)
                .map_err(|error| JsValue::from_str(&error.to_string()))?;
            Ok(if status == 409 {
                Outcome::Conflict(error)
            } else {
                Outcome::Rejected(error)
            })
        } else {
            Err(JsValue::from_str(&format!(
                "Unestablished save outcome: HTTP {status}"
            )))
        }
    }
    match send(&command, cancel).await {
        Ok(outcome) => outcome,
        // A connection failure cannot prove the server did not commit.
        Err(error) => Outcome::Unknown(SaveError::transport(error)),
    }
}
