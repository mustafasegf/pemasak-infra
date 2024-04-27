import { createLazyFileRoute } from '@tanstack/react-router'

export const Route = createLazyFileRoute('/project/$owner/$project/logs')({
  component: () => <div>Hello /project/$owner/$project/logs!</div>
})