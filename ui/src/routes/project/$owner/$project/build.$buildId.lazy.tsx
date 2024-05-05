import { createLazyFileRoute } from '@tanstack/react-router'

export const Route = createLazyFileRoute('/project/$owner/$project/build/$buildId')({
  component: () => <div>Hello /project/$owner/$project/build/$buildId!</div>
})