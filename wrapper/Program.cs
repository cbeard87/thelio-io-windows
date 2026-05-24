using System;
using System.Globalization;
using LibreHardwareMonitor.Hardware;

namespace wrapper
{
    // LibreHardwareMonitor only refreshes a sensor when its hardware (and any
    // sub-hardware) is visited, so traverse the whole tree before each read.
    class UpdateVisitor : IVisitor
    {
        public void VisitComputer(IComputer computer) => computer.Traverse(this);

        public void VisitHardware(IHardware hardware)
        {
            hardware.Update();
            foreach (IHardware sub in hardware.SubHardware)
            {
                sub.Accept(this);
            }
        }

        public void VisitSensor(ISensor sensor) { }
        public void VisitParameter(IParameter parameter) { }
    }

    class Program
    {
        static void Main(string[] args)
        {
            var computer = new Computer
            {
                IsCpuEnabled = true,
                IsGpuEnabled = true,
            };
            computer.Open();
            var visitor = new UpdateVisitor();

            // One reading per line received on stdin; exit cleanly when the parent
            // service closes the pipe (ReadLine returns null).
            string line;
            while ((line = Console.ReadLine()) != null)
            {
                computer.Accept(visitor);

                float max = 0.0f;
                foreach (IHardware hardware in computer.Hardware)
                {
                    max = Math.Max(max, MaxTemperature(hardware));
                }

                // InvariantCulture so the decimal separator is always '.', which is
                // what the Rust side parses regardless of the system locale.
                Console.WriteLine(max.ToString(CultureInfo.InvariantCulture));
            }

            computer.Close();
        }

        static float MaxTemperature(IHardware hardware)
        {
            float max = 0.0f;
            foreach (ISensor sensor in hardware.Sensors)
            {
                if (sensor.SensorType == SensorType.Temperature && sensor.Value.HasValue)
                {
                    max = Math.Max(max, sensor.Value.Value);
                }
            }
            foreach (IHardware sub in hardware.SubHardware)
            {
                max = Math.Max(max, MaxTemperature(sub));
            }
            return max;
        }
    }
}
