use leptos::*;

#[component]
pub fn base(children: Children) -> impl IntoView {
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
                        <div class="navbar bg-base-200 px-8 space-x-2 fixed">
                            <label for="my-drawer" class="btn btn-neutral drawer-button">
                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-6 h-6">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5" />
                                </svg>
                            </label>
                            <a href="/dashboard" class="btn btn-ghost normal-case text-xl">InfraCook</a>
                        </div>
                        <div class="px-8 pt-8 pb-5 flex flex-col justify-center sm:px-12 md:px-24 lg:px-28 xl:mx-auto xl:max-w-6xl w-full min-h-screen">
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
