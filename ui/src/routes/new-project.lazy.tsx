import { createLazyFileRoute } from '@tanstack/react-router';

export const Route = createLazyFileRoute('/new-project')({
  component: NewProject,
})

function NewProject() {
  return (
    <div className="w-full relative min-h-screen">
      <div className="w-full border-b border-slate-600 bg-[#020618] h-24 flex items-center absolute top-0">
        <div className="p-8">
          <h1 className="text-3xl font-semibold">Create a New Project</h1>
        </div>
      </div>
      <div className="h-full mt-6">
      </div>
    </div>
  )
}