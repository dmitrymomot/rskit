use modo_jobs::job;

#[job(queue = "default")]
fn sync_job() -> Result<(), modo::Error> {
    Ok(())
}

fn main() {}
