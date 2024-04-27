import { Badge } from '@/components/ui/badge'
import { createLazyFileRoute, useParams } from '@tanstack/react-router'

export const Route = createLazyFileRoute('/project/$owner/$project/')({
  component: ProjectDashboardIndex
})

function ProjectDashboardIndex() {
  // @ts-ignore
  const { owner, project } = useParams({ strict: false })
  const domain = import.meta.env.VITE_API_URL.match(/((.*):\/\/(.*)\/)/)?.[0]

  return (
    <div className="space-y-4">
      <div className="text-sm space-y-1">
        <h1 className="text-xl font-medium">Project Builds</h1>
        <p>List of all build logs of this project</p>
      </div>
      <p className="text-sm text-blue-400">
        You have not published a build to your project. Please use the following command in your project’s folder to push an existing app to this project.
      </p>
      <div className="w-full p-8 bg-slate-900 rounded-lg">
        <pre>
          git remote add pws {domain}{owner}/{project}
        </pre>
        <pre>
          git branch -M master
        </pre>
        <pre>
          git push pws master
        </pre>
      </div>

      <div
        className="bg-slate-900 border p-8 rounded-lg space-y-4 border-slate-500 hover:border-blue-400 transition-all cursor-pointer"
      >
        <div className="space-y-1">
          <h1 className="text-lg font-semibold">018d8864-d434-8c58-3cac-dc5344a439c2</h1>
          <h2 className="text-sm text-slate-400">Started at Thu, 8 Feb 2024 11:05:25 +0000</h2>
        </div>

        <Badge className="bg-slate-700 hover:bg-slate-700 text-white rounded-full font-medium">
          Pending
        </Badge>
      </div>
    </div>
  )
}