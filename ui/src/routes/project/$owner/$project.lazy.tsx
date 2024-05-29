import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Link, Outlet, createLazyFileRoute, useParams } from "@tanstack/react-router";
import useSWR from "swr";

export const Route = createLazyFileRoute('/project/$owner/$project')({
    component: ProjectDashboard,
})

const apiFetcher = (input: URL | RequestInfo, options?: RequestInit) => {
    return fetch(
        input,
        {
            ...options,
            redirect: "follow",
            credentials: "include",
            headers: {
                "Content-Type": "application/json"
            },
        }
    ).then(res => res.json())
}

function ProjectDashboard() {
    // @ts-ignore
    const { owner, project } = useParams({ strict: false })
    const domain = import.meta.env.VITE_API_URL.match(/((.*):\/\/(.*)\/)/)?.[0].replace(/^http:\/\//, "")

    const { data: builds, isLoading } = useSWR(`${import.meta.env.VITE_API_URL}/project/${owner}/${project}/builds/`, apiFetcher)

    return (
        <div className="w-full relative min-h-screen">
            <div className="w-full border-b border-slate-600 bg-[#020618] h-24 flex items-center absolute top-0">
                <div className="p-8">
                    <h1 className="text-3xl font-semibold">Project Details</h1>
                </div>
            </div>

            <div className="h-full mt-24 space-y-8 overflow-y-auto pb-32">
                <div className="space-y-2 border-b border-slate-600 p-8">
                    <div className="flex items-center space-x-4">
                        {isLoading ? (
                            <Badge className="bg-slate-700 hover:bg-slate-700 text-white text-sm rounded-full font-medium animate-pulse">
                                Loading Status...
                            </Badge>
                        ) : (
                            builds?.data?.filter((build: any) => build.status === "SUCCESSFUL").length > 0 ? (
                                <Badge className="bg-green-700 hover:bg-green-700 text-white text-sm rounded-full font-medium">
                                    Status: Running
                                </Badge>
                            ) : (
                                <Badge className="bg-slate-700 hover:bg-slate-700 text-white text-sm rounded-full font-medium">
                                    Status: Empty
                                </Badge>
                            )
                        )}
                        <h1 className="text-2xl font-semibold">
                            {owner}/{project}
                        </h1>
                    </div>
                    <div className="flex items-center space-x-8">
                        <div className="flex bg-slate-800 p-2 max-w-min rounded-lg gap-2">
                            <Link
                                to="/project/$owner/$project"
                                params={{ owner, project }}
                                className="flex px-4 py-2 rounded-lg items-center hover:bg-slate-900 transition-all"
                                activeProps={{
                                    className: "bg-slate-900"
                                }}
                                activeOptions={{
                                    exact: true,
                                }}
                            >
                                <svg className="mr-1.5" width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                    <path fill-rule="evenodd" clip-rule="evenodd" d="M12 2C6.48 2 2 6.48 2 12C2 17.52 6.48 22 12 22C17.52 22 22 17.52 22 12C22 6.48 17.52 2 12 2ZM12 20C7.59 20 4 16.41 4 12C4 7.59 7.59 4 12 4C16.41 4 20 7.59 20 12C20 16.41 16.41 20 12 20Z" fill="white" />
                                    <path fill-rule="evenodd" clip-rule="evenodd" d="M13.49 11.38C13.92 10.16 13.66 8.74 12.68 7.76C11.57 6.65 9.89 6.46 8.58 7.17L10.93 9.52L9.52 10.93L7.17 8.58C6.46 9.9 6.65 11.57 7.76 12.68C8.74 13.66 10.16 13.92 11.38 13.49L14.79 16.9C14.99 17.1 15.3 17.1 15.5 16.9L16.9 15.5C17.1 15.3 17.1 14.99 16.9 14.79L13.49 11.38Z" fill="white" />
                                </svg>
                                Builds
                            </Link>
                            <Link
                                to="/project/$owner/$project/terminal"
                                params={{ owner, project }}
                                className="flex px-4 py-2 rounded-lg items-center hover:bg-slate-900 transition-all"
                                activeProps={{
                                    className: "bg-slate-900"
                                }}
                            >
                                <svg className="mr-1.5" width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                    <path d="M7 11L9 9L7 7" stroke="#F8FAFC" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                                    <path d="M11 13H15" stroke="#F8FAFC" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                                    <path d="M19 3H5C3.89543 3 3 3.89543 3 5V19C3 20.1046 3.89543 21 5 21H19C20.1046 21 21 20.1046 21 19V5C21 3.89543 20.1046 3 19 3Z" stroke="#F8FAFC" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                                </svg>
                                Terminal
                            </Link>
                            {/* <Link 
                            to="/project/$owner/$project/logs" 
                            params={{owner, project}}
                            className="flex px-4 py-2 rounded-lg items-center hover:bg-slate-900 transition-all"
                            activeProps={{
                                className: "bg-slate-900"
                            }}
                        >
                            <svg className="mr-1.5" width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M19.5 3.5L18 2L16.5 3.5L15 2L13.5 3.5L12 2L10.5 3.5L9 2L7.5 3.5L6 2V16H3V19C3 20.66 4.34 22 6 22H18C19.66 22 21 20.66 21 19V2L19.5 3.5ZM15 20H6C5.45 20 5 19.55 5 19V18H15V20ZM19 19C19 19.55 18.55 20 18 20C17.45 20 17 19.55 17 19V16H8V5H19V19Z" fill="white" />
                                <path d="M15 7H9V9H15V7Z" fill="white" />
                                <path d="M18 7H16V9H18V7Z" fill="white" />
                                <path d="M15 10H9V12H15V10Z" fill="white" />
                                <path d="M18 10H16V12H18V10Z" fill="white" />
                            </svg>
                            Logs
                        </Link> */}
                            <Link
                                to="/project/$owner/$project/settings"
                                params={{ owner, project }}
                                className="flex px-4 py-2 rounded-lg items-center hover:bg-slate-900 transition-all"
                                activeProps={{
                                    className: "bg-slate-900"
                                }}
                            >
                                <svg className="mr-1.5" width="20" height="20" viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg">
                                    <path d="M17.4318 10.98C17.4718 10.66 17.5018 10.34 17.5018 10C17.5018 9.66 17.4718 9.34 17.4318 9.02L19.5418 7.37C19.7318 7.22 19.7818 6.95 19.6618 6.73L17.6618 3.27C17.5718 3.11 17.4018 3.02 17.2218 3.02C17.1618 3.02 17.1018 3.03 17.0518 3.05L14.5618 4.05C14.0418 3.65 13.4818 3.32 12.8718 3.07L12.4918 0.42C12.4618 0.18 12.2518 0 12.0018 0H8.00179C7.75179 0 7.54179 0.18 7.51179 0.42L7.13179 3.07C6.52179 3.32 5.96179 3.66 5.44179 4.05L2.95179 3.05C2.89179 3.03 2.83179 3.02 2.77179 3.02C2.60179 3.02 2.43179 3.11 2.34179 3.27L0.341793 6.73C0.211793 6.95 0.271793 7.22 0.461793 7.37L2.57179 9.02C2.53179 9.34 2.50179 9.67 2.50179 10C2.50179 10.33 2.53179 10.66 2.57179 10.98L0.461793 12.63C0.271793 12.78 0.221793 13.05 0.341793 13.27L2.34179 16.73C2.43179 16.89 2.60179 16.98 2.78179 16.98C2.84179 16.98 2.90179 16.97 2.95179 16.95L5.44179 15.95C5.96179 16.35 6.52179 16.68 7.13179 16.93L7.51179 19.58C7.54179 19.82 7.75179 20 8.00179 20H12.0018C12.2518 20 12.4618 19.82 12.4918 19.58L12.8718 16.93C13.4818 16.68 14.0418 16.34 14.5618 15.95L17.0518 16.95C17.1118 16.97 17.1718 16.98 17.2318 16.98C17.4018 16.98 17.5718 16.89 17.6618 16.73L19.6618 13.27C19.7818 13.05 19.7318 12.78 19.5418 12.63L17.4318 10.98ZM15.4518 9.27C15.4918 9.58 15.5018 9.79 15.5018 10C15.5018 10.21 15.4818 10.43 15.4518 10.73L15.3118 11.86L16.2018 12.56L17.2818 13.4L16.5818 14.61L15.3118 14.1L14.2718 13.68L13.3718 14.36C12.9418 14.68 12.5318 14.92 12.1218 15.09L11.0618 15.52L10.9018 16.65L10.7018 18H9.30179L8.95179 15.52L7.89179 15.09C7.46179 14.91 7.06179 14.68 6.66179 14.38L5.75179 13.68L4.69179 14.11L3.42179 14.62L2.72179 13.41L3.80179 12.57L4.69179 11.87L4.55179 10.74C4.52179 10.43 4.50179 10.2 4.50179 10C4.50179 9.8 4.52179 9.57 4.55179 9.27L4.69179 8.14L3.80179 7.44L2.72179 6.6L3.42179 5.39L4.69179 5.9L5.73179 6.32L6.63179 5.64C7.06179 5.32 7.47179 5.08 7.88179 4.91L8.94179 4.48L9.10179 3.35L9.30179 2H10.6918L11.0418 4.48L12.1018 4.91C12.5318 5.09 12.9318 5.32 13.3318 5.62L14.2418 6.32L15.3018 5.89L16.5718 5.38L17.2718 6.59L16.2018 7.44L15.3118 8.14L15.4518 9.27ZM10.0018 6C7.79179 6 6.00179 7.79 6.00179 10C6.00179 12.21 7.79179 14 10.0018 14C12.2118 14 14.0018 12.21 14.0018 10C14.0018 7.79 12.2118 6 10.0018 6ZM10.0018 12C8.90179 12 8.00179 11.1 8.00179 10C8.00179 8.9 8.90179 8 10.0018 8C11.1018 8 12.0018 8.9 12.0018 10C12.0018 11.1 11.1018 12 10.0018 12Z" fill="white" />
                                </svg>
                                Settings
                            </Link>
                        </div>

                        <a href={builds?.data?.length > 0 ? `http://${owner.replace(".", "-")}-${project}.${domain}` : undefined}>
                            <Button size="lg" className="text-foreground" disabled={builds?.data?.length <= 0}>
                                <svg width="20" height="20" viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg" className="mr-2">
                                    <path d="M15.8333 16.1667H16.1667V15.8333V10.3333H17.1667V15.8333C17.1667 16.5659 16.5659 17.1667 15.8333 17.1667H4.16667C3.42685 17.1667 2.83333 16.567 2.83333 15.8333V4.16667C2.83333 3.43301 3.42685 2.83333 4.16667 2.83333H9.66667V3.83333H4.16667H3.83333V4.16667V15.8333V16.1667H4.16667H15.8333ZM16.1667 8V5.34167V4.53693L15.5976 5.10596L7.64167 13.0619L6.93807 12.3583L14.894 4.40237L15.4631 3.83333H14.6583H12V2.83333H17.1667V8H16.1667Z" fill="#EFF6FF" stroke="#EFF6FF" stroke-width="0.666667" />
                                </svg>

                                View Project
                            </Button>
                        </a>
                    </div>
                </div>
                <div className="px-8">
                    <Outlet />
                </div>
            </div>
        </div>
    )
}
