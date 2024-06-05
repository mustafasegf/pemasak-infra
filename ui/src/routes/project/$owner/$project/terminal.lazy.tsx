import { createLazyFileRoute, useParams } from '@tanstack/react-router'
import { useEffect, useState } from 'react';
import useWebSocket from 'react-use-websocket'

export const Route = createLazyFileRoute('/project/$owner/$project/terminal')({
  component: ProjectDashboardTerminal
})

function ProjectDashboardTerminal() {
  // @ts-ignore
  const { owner, project } = useParams({
    strict: false,
  })

  const [messageHistory, setMessageHistory] = useState<MessageEvent<any>[]>([]);
  // @ts-ignore
  const { sendMessage, lastMessage, readyState } = useWebSocket(`${import.meta.env.VITE_WS_URL}/project/${owner}/${project}/terminal/ws`);

  useEffect(() => {
    if (lastMessage !== null) {
      setMessageHistory((prev) => prev.concat(lastMessage));
    }
  }, [lastMessage]);

  console.log(messageHistory)

  return (
    <div className="space-y-4 w-full">
      <div className="text-sm space-y-1">
        <h1 className="text-xl font-semibold">Project Web Terminal</h1>
        <p className="text-sm">Execute commands directly to your deployed application here.</p>
      </div>

      <div className="w-full p-8 bg-slate-900 rounded-lg">
        {messageHistory.map((message) => (
          <pre key={message.timeStamp} className="w-full">
            {message.data}
          </pre>
        ))}
        <pre className="w-full space-x-4">
          <form onSubmit={(e) => {
            e.preventDefault()
            // @ts-ignore
            const form = new FormData(e.target)
            sendMessage(JSON.stringify({
              message: form.get("message")
            }))
            
            if (form.get("message") === "clear") setMessageHistory([])
            // @ts-ignore
            e.target.reset()
          }}>
            <span>
              &gt;&nbsp;
            </span>
            <input 
              className="bg-transparent !outline-none w-full"
              placeholder="Enter Command"
              name="message"
            />
          </form>
        </pre>
      </div>
    </div>
  )
}