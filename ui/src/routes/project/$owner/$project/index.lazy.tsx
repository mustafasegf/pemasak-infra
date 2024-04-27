import { createLazyFileRoute } from "@tanstack/react-router";

export const Route = createLazyFileRoute('/project/$owner/$project/')({
    component: ProjectDashboard,
})

function ProjectDashboard() {
    return (
        <div className="w-full relative min-h-screen">
        
        </div>
    )
}
