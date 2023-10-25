use leptos::*;

#[component]
pub fn base(children: Children) -> impl IntoView{
    view!{
        <html data-theme="night">
            <head>
                <script src="https://unpkg.com/htmx.org@1.9.6"></script>
                <link rel="icon" type="image/x-icon" href="/assets/favicon.ico" />
                // TODO: change tailwind to use node
                <link href="https://cdn.jsdelivr.net/npm/daisyui@3.8.2/dist/full.css" rel="stylesheet" type="text/css" />
                <script src="https://cdn.tailwindcss.com"></script>
                <link rel="stylesheet" href="/global.css"> 
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
                //TODO: maybe make this optional
                <div class="flex min-h-screen w-full flex-col items-center justify-center circle-bg p-12 lg:p-32">
                    {children()}
                </div>
            </body>
        </html>
    }
}
