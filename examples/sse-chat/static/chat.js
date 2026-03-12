// Auto-scroll to bottom on new messages
var messages = document.getElementById("messages");
if (messages) {
    var observer = new MutationObserver(function () {
        messages.scrollTop = messages.scrollHeight;
    });
    observer.observe(messages, { childList: true });
    messages.scrollTop = messages.scrollHeight;
}
