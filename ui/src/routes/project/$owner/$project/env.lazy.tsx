import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { DialogClose } from '@radix-ui/react-dialog'
import { Pencil1Icon, TrashIcon } from '@radix-ui/react-icons'
import { createLazyFileRoute, useParams } from '@tanstack/react-router'
import React, { useEffect, useState } from 'react'
import { useForm } from 'react-hook-form'
import useSWR, { useSWRConfig } from 'swr'

export const Route = createLazyFileRoute('/project/$owner/$project/env')({
  component: ProjectDashboardEnv
})

function EnvironmentVariable({ envKey, envValue, owner, project }: { envKey: string, envValue: string, owner: string, project: string }) {
  const { mutate } = useSWRConfig()

  async function deleteEnv() {
    await fetch(`${import.meta.env.VITE_API_URL}/project/${owner}/${project}/env/delete`, {
      credentials: "include",
      headers: {
        "Content-Type": "application/json"
      },
      method: "POST",
      body: JSON.stringify({
        key: envKey,
      })
    })
      .finally(() => {
        mutate(`${import.meta.env.VITE_API_URL}/project/${owner}/${project}/env/`)
      })
  }

  return (
    <div className="bg-slate-900 px-6 py-4 rounded-lg grid grid-cols-3 items-center gap-4">
      <div className="text-lg">
        <pre>{envKey}</pre>
      </div>
      <div>
        {envValue}
      </div>
      <div className="flex justify-end space-x-4">
        <ModifyEnvironDialog envKey={envKey} envValue={envValue} owner={owner} project={project}>
          <Button variant="outline" size="lg" className="border-primary bg-transparent text-primary hover:bg-primary">
            <Pencil1Icon className="w-5 h-5" />
          </Button>
        </ModifyEnvironDialog>
        <Button onClick={deleteEnv} variant="outline" size="lg" className="border-red-500 bg-transparent text-red-500 hover:bg-red-500 hover:text-white">
          <TrashIcon className="w-6 h-6" />
        </Button>
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

function ModifyEnvironDialog({ owner, project, envKey, envValue, children }: { owner: string, project: string, envKey?: string, envValue?: string, children: React.ReactNode }) {
  const {
    handleSubmit,
    register,
    setValue,
  } = useForm()

  const [open, setOpen] = useState(false)
  const { mutate } = useSWRConfig()
  const isCreation = Boolean(envKey)

  useEffect(() => {
    setValue("key", envKey)
    setValue("value", envValue)
  }, [envKey, envValue])

  async function submitHandler(data: any) {
    await fetch(`${import.meta.env.VITE_API_URL}/project/${owner}/${project}/env`, {
      credentials: "include",
      headers: {
        "Content-Type": "application/json"
      },
      method: "POST",
      body: JSON.stringify({
        key: data.key,
        value: data.value,
      })
    })
      .then(() => setOpen(false))
      .finally(() => {
        mutate(`${import.meta.env.VITE_API_URL}/project/${owner}/${project}/env/`)
      })
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger>
        {children}
      </DialogTrigger>
      <DialogContent className="text-white">
        <DialogHeader>
          <DialogTitle>{!isCreation ? "Create" : "Modify"} Environment Variable</DialogTitle>
        </DialogHeader>
        <form className="space-y-2" onSubmit={handleSubmit(submitHandler)}>
          <div className="space-y-2">
            <label>Key</label>
            <Input disabled={isCreation} className="bg-slate-900 border-slate-600 bg-opacity-90" {...register("key")} />
          </div>
          <div className="space-y-2">
            <label>Value</label>
            <Input className="bg-slate-900 border-slate-600 bg-opacity-90" {...register("value")} />
          </div>
          <DialogFooter>
            <DialogClose>
              <Button size="lg" className="bg-red-600 text-foreground hover:bg-red-700">
                Cancel
              </Button>
            </DialogClose>
            <Button type="submit" size="lg" className="text-foreground">
              Create
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function ProjectDashboardEnv() {
  // @ts-ignore
  const { owner, project } = useParams({ strict: false })
  const { data, isLoading } = useSWR(`${import.meta.env.VITE_API_URL}/project/${owner}/${project}/env/`, apiFetcher)

  return (
    <div className="space-y-4 w-full">
      <div className="flex justify-between">
        <div className="text-sm space-y-1">
          <h1 className="text-xl font-semibold">Project Environment Variables</h1>
          <p className="text-sm">Set environment variables for your application here.</p>
        </div>
        <ModifyEnvironDialog owner={owner} project={project}>
          <Button size="lg" className="text-foreground">
            New Environment Variable
          </Button>
        </ModifyEnvironDialog>
      </div>

      <div className="space-y-4">
        {!isLoading && (
          Object.entries(data?.env ?? {}).map((item) => {
            return (
              <EnvironmentVariable project={project} owner={owner} envKey={item[0]} envValue={item[1] as string} />
            )
          })
        )}
      </div>
    </div>
  )
}