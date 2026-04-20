#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (state, listener) = backend::bootstrap().await?;

    if state.config.llm.enabled {
        tokio::spawn(backend::llm::job_runner::run_semantic_job_runner(
            state.clone(),
        ));
    }

    axum::serve(listener, backend::app(state)).await?;
    Ok(())
}
