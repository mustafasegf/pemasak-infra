import { createLazyFileRoute, useParams } from '@tanstack/react-router'
import useSWR from 'swr'

export const Route = createLazyFileRoute('/project/$owner/$project/build/$buildId')({
  component: ProjectViewBuildLog
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

function ProjectViewBuildLog() {
  // @ts-ignore
  const { owner, project, buildId } = useParams({ strict: false })

  const { data: build, isLoading } = useSWR(`${import.meta.env.VITE_API_URL}/project/${owner}/${project}/builds/${buildId}`, apiFetcher)

  console.log(
    build, isLoading
  )

  return (
    <div className="space-y-4">
      <div className="text-sm space-y-1">
        <h1 className="text-xl font-medium">Build Logs</h1>
        <p>Build ID: {build?.id}</p>
      </div>
      <div className="w-full p-8 bg-slate-900 rounded-lg max-h-96 overflow-y-auto overflow-x-hidden">
        <pre className="w-full space-x-4 whitespace-pre-wrap">
          {build?.logs}
        </pre>
      </div>
    </div>
  )
}