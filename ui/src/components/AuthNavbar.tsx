import { FC, ReactElement } from "react";

export default function AuthNavbar(): ReactElement<FC> {
    return (
        <div className="fixed w-full flex justify-start items-center px-6 py-3 bg-[#020618] border border-transparent border-b-slate-600">
            <div className="flex space-x-4 items-center">
                <img className="w-12 h-12" src="/InfraCook.png" />
                <h1 className="italic text-lg font-medium">
                    PWS - Pacil Web Service
                </h1>
            </div>
        </div>
    )
}