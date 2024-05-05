import { Button } from '@/components/ui/button'
import { createLazyFileRoute } from '@tanstack/react-router'

export const Route = createLazyFileRoute('/project/$owner/$project/settings')({
  component: ProjectDashboardSettings
})

function ProjectDashboardSettings() {
  return (
    <div className="space-y-4 w-full">
      <div className="text-sm space-y-1">
        <h1 className="text-xl font-semibold">Project Settings</h1>
        <p className="text-sm">List of all the possible settings you can do in this project</p>
      </div>
      <div className="w-full space-y-4">
        <div>
          <h1 className="font-medium">Project Controls</h1>
          <p className="text-sm">Actions that you can take in this project</p>
        </div>
        <div className="flex space-x-4">
          <Button className="bg-red-600 text-foreground hover:bg-red-700">
            <svg width="20" height="20" className="mr-1" viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg">
              <path d="M6.81462 9.6643L6.57837 9.90056L6.81518 10.1363L8.35337 11.6672L6.82296 13.1976L6.58725 13.4333L6.82296 13.669L7.99796 14.844L8.23366 15.0797L8.46936 14.844L10.0003 13.3131L11.5313 14.844L11.767 15.0797L12.0027 14.844L13.1777 13.669L13.4134 13.4333L13.1777 13.1976L11.6467 11.6667L13.1777 10.1357L13.4134 9.9L13.1777 9.6643L12.0027 8.4893L11.767 8.2536L11.5313 8.4893L9.99977 10.0208L8.46047 8.48874L8.22477 8.25415L7.98962 8.4893L6.81462 9.6643ZM12.6813 3.56904L12.7789 3.66667H12.917H15.5003V4.66667H4.50033V3.66667H7.08366H7.22173L7.31936 3.56904L8.05506 2.83333H11.9456L12.6813 3.56904ZM6.66699 17.1667C5.93442 17.1667 5.33366 16.5659 5.33366 15.8333V6.16667H14.667V15.8333C14.667 16.5659 14.0662 17.1667 13.3337 17.1667H6.66699Z" fill="white" stroke="white" stroke-width="0.666667" />
            </svg>
            Delete Project
          </Button>
          <Button className="bg-transparent text-red-400 border border-red-400 hover:text-white hover:bg-red-400 group">
            <svg width="20" height="20" className="mr-1 !fill-current !stroke-current" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg">
              <path d="M12.1667 9.16659V9.49992H12.5H13.3333C15.4492 9.49992 17.1667 11.2173 17.1667 13.3333V18.8333H2.83333V13.3333C2.83333 11.2173 4.55076 9.49992 6.66667 9.49992H7.5H7.83333V9.16659V2.49992C7.83333 1.76735 8.43409 1.16659 9.16667 1.16659H10.8333C11.5659 1.16659 12.1667 1.76735 12.1667 2.49992V9.16659ZM15.8333 17.8333H16.1667V17.4999V13.3333C16.1667 11.7742 14.8924 10.4999 13.3333 10.4999H6.66667C5.10757 10.4999 3.83333 11.7742 3.83333 13.3333V17.4999V17.8333H4.16667H5.83333H6.16667V17.4999V14.9999C6.16667 14.7257 6.39243 14.4999 6.66667 14.4999C6.94091 14.4999 7.16667 14.7257 7.16667 14.9999V17.4999V17.8333H7.5H9.16667H9.5V17.4999V14.9999C9.5 14.7257 9.72576 14.4999 10 14.4999C10.2742 14.4999 10.5 14.7257 10.5 14.9999V17.4999V17.8333H10.8333H12.5H12.8333V17.4999V14.9999C12.8333 14.7257 13.0591 14.4999 13.3333 14.4999C13.6076 14.4999 13.8333 14.7257 13.8333 14.9999V17.4999V17.8333H14.1667H15.8333Z" stroke-width="0.666667" />
            </svg>
            Clear Database
          </Button>
        </div>
      </div>
    </div>
  )
}