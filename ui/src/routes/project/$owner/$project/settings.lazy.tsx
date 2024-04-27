import { createLazyFileRoute } from '@tanstack/react-router'

export const Route = createLazyFileRoute('/project/$owner/$project/settings')({
  component: () => <div>Hello /project/$owner/$project/settings!</div>
})