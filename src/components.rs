use leptos::*;

#[component]
pub fn base(
    children: Children,
    #[prop(optional)] class: String,
    #[prop(optional)] is_logged_in: bool,
) -> impl IntoView {
    view! {
        <html data-theme="night">
            <head>
                <script src="https://unpkg.com/htmx.org@1.9.6"></script>
                <script src="https://unpkg.com/hyperscript.org@0.9.12"></script>
                <script src="https://unpkg.com/htmx.org/dist/ext/ws.js"></script>
                <link rel="icon" type="image/x-icon" href="/assets/favicon.ico" />
                // TODO: change tailwind to use node
                <link href="https://cdn.jsdelivr.net/npm/daisyui@3.8.2/dist/full.css" rel="stylesheet" type="text/css" />
                <script src="https://cdn.tailwindcss.com"></script>
                <link rel="stylesheet" href="/assets/global.css" />
            </head>
            <body>
                // need this in body so body exist
                <script> {"
                    document.body.addEventListener('htmx:beforeSwap', function(evt) {{
                      let status = evt.detail.xhr.status;
                      if (status === 500 || status === 422 || status === 409 || status === 400) {{
                        evt.detail.shouldSwap = true;
                        evt.detail.isError = false;
                      }}
                    }});
                "}</script>

                <div class="drawer circle-bg">
                    <input id="my-drawer" type="checkbox" class="drawer-toggle" />
                    <div class="drawer-content">
                        <div class="w-full fixed z-50">
                            <div hx-boost="true" class="navbar bg-transparent backdrop-blur-sm px-8 py-6 space-x-2 mx-auto xl:max-w-6xl w-full justify-between">
                                <a href="/dashboard" class="btn btn-ghost normal-case text-xl px-0">
                                    <img class="w-12 h-12" src="/assets/InfraCook.png"></img>
                                </a>
                                {move || {
                                    match is_logged_in {
                                        true => view! { <><a class="btn btn-error btn-outline btn-sm" href="/logout">Sign Out</a></> },
                                        false => view! { <></> }
                                    }
                                }}
                            </div>
                        </div>
                        <div class={"px-8 pt-32 pb-5 flex flex-col xl:mx-auto xl:max-w-6xl w-full min-h-screen ".to_string() + &class}>
                            {children()}
                        </div>
                    </div>
                    <div class="drawer-side" hx-boost="true">
                        <label for="my-drawer" aria-label="close sidebar" class="drawer-overlay"></label>
                        <ul class="menu p-4 w-80 min-h-full bg-base-200 text-base-content">
                            <li><a href="/dashboard">Dashboard</a></li>
                            <li><a href="/owner">Owner Groups</a></li>
                        </ul>
                    </div>
                </div>
            </body>
        </html>
    }
}
