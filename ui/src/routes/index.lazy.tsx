import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import Spinner from '@/components/ui/spinner';
import { Link, createLazyFileRoute } from '@tanstack/react-router';
import useSWR from 'swr';


export const Route = createLazyFileRoute('/')({
  component: Index,
})

function NoProject() {
  return (
    <div className="h-full flex flex-col justify-center items-center">
      <img src="/web/no-project.svg" />
      <div className="flex flex-col justify-center items-center space-y-4">
        <div className="space-y-2 text-center">
          <h1 className="text-3xl font-semibold">You currently have no projects</h1>
          <h2 className="text-lg">Let's create one easily</h2>
        </div>
        <Link href="/create-project" to="/create-project">
          <Button size="lg" className="text-white">
            Create New Project
          </Button>
        </Link>
      </div>
    </div>
  )
}

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

function Index() {
  const { data: projects, isLoading } = useSWR(`${import.meta.env.VITE_API_URL}/dashboard/project/`, apiFetcher)

  return (
    <div className="w-full relative min-h-screen">
      <div className="w-full border-b border-slate-600 bg-[#020618] h-24 flex items-center absolute top-0">
        <div className="p-8">
          <h1 className="text-3xl font-semibold">Home</h1>
        </div>
      </div>

      <div className="h-full mt-24 p-8 space-y-8 overflow-y-auto pb-32">
        {isLoading || !projects?.data?.length ? <NoProject /> : (
          <>
            <h1 className="font-semibold text-2xl">Project List</h1>
            <div className="grid grid-cols-2 gap-8">
              {projects?.data?.map((item: any) => (
                <Link
                  href={`/web/${item.owner_name}/${item.name}/`}
                  to="/project/$owner/$project/"
                  params={{
                    owner: item.owner_name,
                    project: item.name
                  }}
                  className="bg-slate-900 border p-8 rounded-lg space-y-4 border-slate-500 hover:border-blue-400 transition-all cursor-pointer"
                >
                  <div className="space-y-1">
                    <h1 className="text-lg font-semibold">{item.owner_name}/{item.name}</h1>
                    <h2 className="text-sm text-blue-400">{item.id}</h2>
                  </div>

                  <Badge className="bg-slate-700 hover:bg-slate-700 text-white rounded-full font-medium">
                    Status: Empty
                  </Badge>
                </Link>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  )
}