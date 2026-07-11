//! Human account flow: signup/login (session token) → `human` (register as a
//! human instance in a project, minting a normal instance token).

use std::io::IsTerminal;

use crate::config;

fn read_password() -> anyhow::Result<String> {
    if std::io::stdin().is_terminal() {
        Ok(rpassword::prompt_password("password: ")?)
    } else {
        // Piped stdin (scripts, tests): read a line, no echo concerns.
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        Ok(line.trim_end_matches('\n').to_string())
    }
}

fn prompt_password(confirm: bool) -> anyhow::Result<String> {
    let password = read_password()?;
    if confirm && std::io::stdin().is_terminal() {
        let again = rpassword::prompt_password("confirm password: ")?;
        if password != again {
            anyhow::bail!("passwords don't match");
        }
    }
    Ok(password)
}

pub async fn signup(email: String, display_name: String, business: bool) -> anyhow::Result<()> {
    let password = prompt_password(true)?;
    let resp = config::http_client()
        .post(format!("{}/v1/auth/signup", config::server_url()))
        .json(&kore_protocol::api::SignupRequest { email, password, display_name, business })
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("signup failed ({}): {}", resp.status(), resp.text().await?);
    }
    let body: kore_protocol::api::SignupResponse = resp.json().await?;
    // L9: individual signups get no session until the email is verified.
    match &body.session_token {
        Some(t) => {
            config::save_session(t)?;
            println!("signed up — org '{}', logged in", body.org_name);
        }
        None => println!("signed up — org '{}'; verify your email, then log in", body.org_name),
    }
    if let Some(t) = &body.dev_token {
        println!("dev verify token (mailer unconfigured): {t}");
    }
    if let Some(secret) = body.org_reg_secret {
        println!(
            "\nyour org registration secret (shown ONCE, save it — agents need it):\n  export KORE_REG_SECRET={secret}"
        );
    }
    Ok(())
}

pub async fn login(email: String) -> anyhow::Result<()> {
    let password = prompt_password(false)?;
    let resp = config::http_client()
        .post(format!("{}/v1/auth/login", config::server_url()))
        .json(&kore_protocol::api::LoginRequest { email, password })
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("login failed ({}): {}", resp.status(), resp.text().await?);
    }
    let body: kore_protocol::api::LoginResponse = resp.json().await?;
    config::save_session(&body.session_token)?;
    println!("logged in as {} (org '{}')", body.display_name, body.org_name);
    Ok(())
}

/// Register the logged-in human as an instance; stores the instance token so
/// send/list/listen work as this identity. The name comes back from the
/// server — it's the account's owner_name (DU-S2), never client-chosen.
pub async fn human(project: Option<String>) -> anyhow::Result<()> {
    let session = config::load_session()?;
    let resp = config::http_client()
        .post(format!("{}/v1/auth/register-human", config::server_url()))
        .bearer_auth(session)
        .json(&kore_protocol::api::RegisterHumanRequest { project: project.clone() })
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("register-human failed ({}): {}", resp.status(), resp.text().await?);
    }
    let body: kore_protocol::api::RegisterResponse = resp.json().await?;
    config::save_token(&body.token, project.as_deref())?;
    let name = &body.name;
    match &project {
        Some(p) => println!("you are '{name}' in project '{p}' — export KORE_PROJECT={p} to use this identity"),
        None => println!("you are '{name}'"),
    }
    Ok(())
}
