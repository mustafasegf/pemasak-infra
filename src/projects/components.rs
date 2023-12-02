use leptos::*;

#[component]
pub fn project_header(
    #[prop()]
    owner: String,
    #[prop()]
    project: String,
    #[prop()]
    domain: String,
) -> impl IntoView {
    view! {
        <div class="flex items-center justify-between mb-6">
            <div class="flex flex items-center">
                <div class="flex flex-col justify-center space-y-1">
                    <p class="font-bold text-xl">{&owner}"/"{&project}</p>
                </div>
            </div>
            <div class="flex space-x-4">
                <a href={format!("/{}/{}/preferences", &owner, &project)} hx-boost="true">
                    <button class="btn btn-sm btn-outline btn-secondary gap-1">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M10.5 6h9.75M10.5 6a1.5 1.5 0 11-3 0m3 0a1.5 1.5 0 10-3 0M3.75 6H7.5m3 12h9.75m-9.75 0a1.5 1.5 0 01-3 0m3 0a1.5 1.5 0 00-3 0m-3.75 0H7.5m9-6h3.75m-3.75 0a1.5 1.5 0 01-3 0m3 0a1.5 1.5 0 00-3 0m-9.75 0h9.75" />
                        </svg>
                        Settings
                    </button>
                </a>
                <a href={format!("/{}/{}/terminal", &owner, &project)}>
                    <button class="btn btn-sm btn-outline btn-accent gap-1">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-6 h-6">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M6.75 7.5l3 2.25-3 2.25m4.5 0h3m-9 8.25h13.5A2.25 2.25 0 0021 18V6a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 6v12a2.25 2.25 0 002.25 2.25z" />
                        </svg>                  
                        Terminal
                    </button>
                </a>
                <a href={format!("http://{}-{}.{}", &owner, &project, &domain)} target="_blank" rel="noreferrer">
                    <button class="btn btn-sm btn-outline btn-primary gap-1">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5"><path stroke-linecap="round" stroke-linejoin="round" d="M15.75 9V5.25A2.25 2.25 0 0013.5 3h-6a2.25 2.25 0 00-2.25 2.25v13.5A2.25 2.25 0 007.5 21h6a2.25 2.25 0 002.25-2.25V15m3 0l3-3m0 0l-3-3m3 3H9"></path></svg>
                        Open
                    </button>
                </a>
            </div>
        </div>
    }
}