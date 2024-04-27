import { createLazyFileRoute } from '@tanstack/react-router'

export const Route = createLazyFileRoute('/project/$owner/$project/')({
  component: () => <div>Hello /project/$owner/$project/!</div>
})