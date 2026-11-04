//! Mistral Vibe subscription usage from the public billing endpoints used by
//! the official Vibe CLI. The credential is resolved locally and never sent to
//! the renderer or persisted in the dashboard cache.

use super::{require_success, send_with_one_retry, FetchError, MSG_LOGIN_REQUIRED};
use crate::config::Config;
use crate::models::MistralService;
use crate::secrets;
use crate::util::{clamp_percent, fmt_local, parse_datetime_str};
use anyhow::{bail, Context};
use reqwest::{Client, RequestBuilder};
use serde_json::Value;

const SUBSCRIPTION_URL: &str = "https://api.mistral.ai/v1/billing/subscription";
const BUDGET_URL: &str = "https://api.mistral.ai/v1/billing/budget";

pub async fn fetch(_config: &Config, client: &Client) -> Result<MistralService, FetchError> {
    let key = secrets::mistral_key().ok_or(FetchError::Auth {
        message: MSG_LOGIN_REQUIRED,
    })?;

    let budget = send_with_one_retry(|| request(client, BUDGET_URL, &key))
        .await
        .map_err(FetchError::Other)?;
    require_success(&budget, "Mistral Vibe budget")?;

    // Subscription metadata is useful but not required to render the allowance.
    let subscription = send_with_one_retry(|| request(client, SUBSCRIPTION_URL, &key))
        .await
        .ok()
        .filter(|response| response.is_success())
        .map(|response| response.body);

    parse_usage(&budget.body, subscription.as_deref()).map_err(FetchError::Other)
}

fn request(client: &Client, url: &str, key: &str) -> RequestBuilder {
    client
        .get(url)
        .bearer_auth(key)
        .header("Accept", "application/json")
}

pub(crate) fn parse_usage(
    budget_body: &str,
    subscription_body: Option<&str>,
) -> anyhow::Result<MistralService> {
    let root: Value = serde_json::from_str(budget_body).context("parse Mistral Vibe budget")?;
    let budget = root
        .get("vibe_budget")
        .or_else(|| root.get("vibeBudget"))
        .filter(|value| !value.is_null())
        .context("Mistral Vibe budget missing vibe_budget")?;

    let usage = number(
        budget
            .get("usage_percentage")
            .or_else(|| budget.get("usagePercentage")),
    )
    .context("Mistral Vibe budget missing usage percentage")?;
    if !usage.is_finite() {
        bail!("Mistral Vibe usage percentage is not finite");
    }

    let reset = budget
        .get("reset_at")
        .or_else(|| budget.get("resetAt"))
        .and_then(Value::as_str)
        .and_then(parse_datetime_str)
        .map(fmt_local);

    let budget_payg = budget
        .get("payg_enabled")
        .or_else(|| budget.get("paygEnabled"))
        .and_then(Value::as_bool);

    let (plan, subscription_payg) = subscription_body
        .and_then(|body| serde_json::from_str::<Value>(body).ok())
        .map(|value| {
            let plan = value
                .get("plan_name")
                .or_else(|| value.get("planName"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let payg = value
                .get("vibe_payg_enabled")
                .or_else(|| value.get("vibePaygEnabled"))
                .and_then(Value::as_bool);
            (plan, payg)
        })
        .unwrap_or((None, None));

    Ok(MistralService {
        status: "NOMINAL".into(),
        from_cache: false,
        data_may_be_stale: false,
        plan,
        monthly_percent: Some(clamp_percent(usage)),
        monthly_reset_local: reset,
        payg_enabled: budget_payg.or(subscription_payg),
    })
}

fn number(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|number| number as f64))
        .or_else(|| value.as_u64().map(|number| number as f64))
        .or_else(|| value.as_str()?.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::parse_usage;

    #[test]
    fn parses_official_budget_shape() {
        let service = parse_usage(
            r#"{
                "vibe_budget": {
                    "usage_percentage": 37.25,
                    "initial_budget": 20,
                    "reset_at": "2026-12-01T00:00:00Z",
                    "payg_enabled": false
                }
            }"#,
            Some(r#"{"plan_name":"Education","vibe_payg_enabled":false}"#),
        )
        .expect("valid budget");

        assert_eq!(service.monthly_percent, Some(37.25));
        assert_eq!(service.plan.as_deref(), Some("Education"));
        assert_eq!(service.payg_enabled, Some(false));
        assert!(service.monthly_reset_local.is_some());
    }

    #[test]
    fn clamps_out_of_range_percentage() {
        let service = parse_usage(
            r#"{"vibe_budget":{"usage_percentage":120}}"#,
            None,
        )
        .expect("valid budget");

        assert_eq!(service.monthly_percent, Some(100.0));
    }

    #[test]
    fn missing_vibe_budget_is_visible_as_parser_failure() {
        assert!(parse_usage(r#"{"vibe_budget":null}"#, None).is_err());
    }
}
