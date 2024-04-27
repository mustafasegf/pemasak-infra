import { createLazyFileRoute } from '@tanstack/react-router'

export const Route = createLazyFileRoute('/project/$owner/$project/terminal')({
  component: () => <div>Hello /project/$owner/$project/terminal!</div>
})