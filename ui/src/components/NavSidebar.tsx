import { PersonIcon, PlusIcon } from "@radix-ui/react-icons";
import { FC, ReactElement } from "react";
import { Button } from "./ui/button";

export interface NavSidebarProps {
  className: string
}

export default function NavSidebar({ className }: NavSidebarProps): ReactElement<FC<NavSidebarProps>> {
  return (
    <div className={`${className} border-r h-full min-h-screen border-slate-600 bg-[#020618]`}>
      <div className="flex space-x-4 items-center px-6 py-4">
        <img className="w-12 h-12" src="/InfraCook.png" />
        <h1 className="italic text-lg font-medium">
          PWS - Pacil Web Service
        </h1>
      </div>
      <hr className="border-slate-600" />
      <div className="flex flex-col items-center justify-center px-6 py-4">
        <div className="flex items-center space-x-4 w-full">
          <PersonIcon className="h-6 w-6" />
          <div>
            <h1
              className="font-bold truncate"
            >
              Stefanus Ndaru Wedhatama
            </h1>
            <p className="text-slate-600">
              stefanus.ndaru
            </p>
          </div>
        </div>
      </div>
      <hr className="border-slate-600" />
      <div className="flex flex-col items-center justify-center px-6 py-4">
      </div>
      <hr className="border-slate-600" />
      <div className="flex flex-col items-center justify-center px-6 py-4">
        <Button variant="outline" size="lg" className="w-full space-x-2 border-primary text-primary hover:bg-primary">
          <PlusIcon className="mr-2 h-4 w-4" /> Create New Project
        </Button>
      </div>
    </div>
  )
}