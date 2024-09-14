import { createLazyFileRoute, useParams } from '@tanstack/react-router'
import useSWR from 'swr'

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

function Logs() {
  // @ts-ignore
  const { owner, project } = useParams({ strict: false })
  const { data } = useSWR(`${import.meta.env.VITE_API_URL}/project/${owner}/${project}/logs`, apiFetcher)

  return (
    <div className="space-y-4">
      <div className="text-sm space-y-1">
        <h1 className="text-xl font-semibold">Project Logs</h1>
        <p className="text-sm">Last 100 lines from your deployed project container</p>
      </div>
      <div className="w-full p-8 bg-slate-900 rounded-lg max-h-96 overflow-y-auto overflow-x-hidden">
        <pre className="w-full space-x-4 whitespace-pre-wrap">
          {data?.logs}
        </pre>
      </div>
    </div>
  )
}

export const Route = createLazyFileRoute('/project/$owner/$project/logs')({
  component: () => <Logs />
})