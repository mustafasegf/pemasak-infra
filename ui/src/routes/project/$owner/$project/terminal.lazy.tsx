import { createLazyFileRoute } from '@tanstack/react-router'

export const Route = createLazyFileRoute('/project/$owner/$project/terminal')({
  component: ProjectDashboardTerminal
})

function ProjectDashboardTerminal() {
  return (
    <div className="space-y-4 w-full">
      <div className="text-sm space-y-1">
        <h1 className="text-xl font-semibold">Project Web Terminal</h1>
        <p className="text-sm">Execute commands directly to your deployed application here.</p>
      </div>

      <div className="w-full p-8 bg-slate-900 rounded-lg">
        <pre className="w-full space-x-4">
          <span>
            &gt;
          </span>
          <input 
            className="bg-transparent !outline-none w-full"
            placeholder="Enter Command"
          />
        </pre>
      </div>
    </div>
  )
}