use gpui::{App, PromptLevel};
use tokio::sync::mpsc::Receiver;

pub fn listen(mut requests: Receiver<host::WriteApproval>, cx: &mut App) {
  cx.spawn(async move |cx| {
    while let Some(request) = requests.recv().await {
      let prompt = cx
        .update(|cx| {
          let window = cx.windows().first().copied()?;
          window
            .update(cx, |_, window, cx| {
              window.prompt(
                PromptLevel::Warning,
                &format!("Allow this operation on {}?", request.connection),
                Some(&request.detail),
                &["Cancel", "Run"],
                cx,
              )
            })
            .ok()
        })
        .ok()
        .flatten();
      let approved = match prompt {
        Some(prompt) => prompt.await.ok() == Some(1),
        None => false,
      };
      let _ = request.answer.send(approved);
    }
  })
  .detach();
}
