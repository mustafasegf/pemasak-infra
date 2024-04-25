import { Button } from '@/components/ui/button';
import { Link, createLazyFileRoute } from '@tanstack/react-router';

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

function Index() {
  return (
    <div className="w-full relative min-h-screen">
      <div className="w-full border-b border-slate-600 bg-[#020618] h-24 flex items-center absolute top-0">
        <div className="p-8">
          <h1 className="text-3xl font-semibold">Home</h1>
        </div>
      </div>
      <div className="h-full mt-6">
        <NoProject />
      </div>
    </div>
  )
}